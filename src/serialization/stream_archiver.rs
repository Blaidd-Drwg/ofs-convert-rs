use std::any::{type_name, Any, TypeId};
use std::mem::size_of;
use std::rc::Rc;

use anyhow::Result;

use crate::allocator::{AllocatedClusterIdx, AllocatedReader, Allocator};

type Page = [u8];
type PageIdx = Option<AllocatedClusterIdx>;

pub struct StreamArchiver<'a> {
    /// The index to the first written page. None if
    /// - no page has yet been written, or
    /// - `self.previous_page_idx == self.head` (that is a workaround because PageIdx is not Clone)
    head: PageIdx,
    previous_page_idx: PageIdx, // SAFETY: must not be leaked outside of `self`!
    /// The content of the current page that has yet to be written.
    current_page: Vec<u8>,
    page_size: usize,
    position_in_current_page: usize,
    allocator: Rc<Allocator<'a>>,
}

#[derive(Copy, Clone)]
struct Header {
    pub len: usize,
    pub type_id: TypeId,
}

impl<'a> StreamArchiver<'a> {
    /// `page_size` must be greater than or equal to `size_of::<PageIdx>() + size_of::<T>()` for every type `T` that
    /// will be archived. PANICS: Panics if `page_size < size_of::<PageIdx>() + size_of::<Header>()`.
    pub fn new(allocator: Rc<Allocator<'a>>, page_size: usize) -> Self {
        assert!(page_size >= size_of::<PageIdx>() + size_of::<Header>());

        Self {
            head: None,
            current_page: vec![0; page_size],
            previous_page_idx: None,
            page_size,
            position_in_current_page: size_of::<PageIdx>(),
            allocator,
        }
    }

    pub fn into_reader(mut self) -> Result<(Reader<'a>, Allocator<'a>)> {
        self.finalize()?;
        self.write_page()?;
        let allocator = Rc::try_unwrap(self.allocator).expect(
            "StreamArchiver cannot take ownership of its allocator, somebody else still has a reference to it.",
        );
        let (allocated_reader, new_allocator) = allocator.split_into_reader();
        let head = self.head.or(self.previous_page_idx);
        Ok((Reader::new(head, self.page_size, allocated_reader), new_allocator))
    }

    /// PANICS: Panics if `size_of::<PageIdx>() + size_of::<T>() > self.page_size`
    pub fn archive<T>(&mut self, objects: Vec<T>) -> Result<()>
    where T: Any {
        let header = Header { len: objects.len(), type_id: TypeId::of::<T>() };
        unsafe {
            // SAFETY: Safe assuming the archive is consistent so far.
            self.add_object(header)?;
            for object in objects {
                // SAFETY: Safe because we are adding `header.len` objects with type ID `header.type_id`.
                self.add_object(object)?;
            }
        }
        Ok(())
    }

    /// Marks the end of the archive. A call to `Reader::next` attempting to access any objects beyond the end of the
    /// archive will panic.
    fn finalize(&mut self) -> Result<()> {
        enum End {} // not accessible from outside this function, so a call attempting to read `header` will always panic
        let header = Header { len: 0, type_id: TypeId::of::<End>() };
        unsafe { self.add_object(header) }
    }

    fn previous_page_mut(&mut self) -> Option<&mut Page> {
        Some(self.allocator.cluster_mut(self.previous_page_idx.as_mut()?))
    }

    fn page_mut(&self, page_idx: &mut PageIdx) -> Option<&mut Page> {
        Some(self.allocator.cluster_mut(page_idx.as_mut()?))
    }

    /// Never returns `Ok(None)`
    fn allocate_page(&self) -> Result<PageIdx> {
        Ok(Some(self.allocator.allocate_one()?))
    }

    /// SAFETY: To avoid aliasing, the caller must ensure that the original and the clone are not used to access a
    /// cluster simultaneously.
    unsafe fn clone_page_idx(page_idx: &PageIdx) -> PageIdx {
        page_idx.as_ref().map(|idx| idx.clone())
    }

    fn write_page(&mut self) -> Result<()> {
        let mut page_idx = self.allocate_page()?;
        self.page_mut(&mut page_idx).unwrap().copy_from_slice(&self.current_page);

        // if the current page is not the head, write the current page's index into the previous page's next pointer
        if let Some(previous_page) = self.previous_page_mut() {
            let ptr = previous_page.as_mut_ptr() as *mut PageIdx;
            unsafe {
                // SAFETY: Safe because `page_idx_clone` is immediately written to a page, and since `page_idx` is not
                // leaked outside of `self`, `page_idx_clone` can only be read after `page_idx` has been dropped.
                let page_idx_clone = Self::clone_page_idx(&page_idx);
                // SAFETY: Safe because `ptr` points to `previous_page`, which we have a mutable borrow for.
                ptr.write_unaligned(page_idx_clone);
            }
        }

        // This is only the case if the previous page is also the head. Since we're replacing the previous page now but
        // the head still stays the same, we move it to the head.
        if self.previous_page_idx.is_some() && self.head.is_none() {
            std::mem::swap(&mut self.previous_page_idx, &mut page_idx);
            self.head = page_idx;
        } else {
            self.previous_page_idx = page_idx;
        }

        // SAFETY: Safe because the content of `self.current_page` has already been written out.
        unsafe {
            self.reset_page();
        }
        Ok(())
    }

    /// SAFETY: By resetting the current page, this method may delete data needed to keep the archive consistent. The
    /// caller must ensure that such data was already written into an allocated page.
    unsafe fn reset_page(&mut self) {
        self.current_page.fill(0);
        // this is the last page for now, set the next page index to None
        let ptr = self.current_page.as_mut_ptr() as *mut PageIdx;
        // SAFETY: Safe because we have a mutable borrow on `self.current_page` and `self.current_page.len() >=
        // size_of::<PageIdx>()`.
        ptr.write_unaligned(None);
        self.position_in_current_page = size_of::<PageIdx>();
    }

    /// SAFETY: Only safe if consistent with the preceding header. I.e. either:
    /// 1) The preceding header `h` is followed by `h.len` objects. Then `object must be of type `Header`; or
    /// 2) The preceding header `h` is followed by fewer than `h.len` objects. Then `T` must have the ID `h.type_id`.
    /// PANICS: Panics if `size_of::<PageIdx>() + size_of::<T>() > self.page_size`
    unsafe fn add_object<T>(&mut self, object: T) -> Result<()> {
        if self.space_left_in_page() < size_of::<T>() {
            self.write_page()?;
        }
        assert!(
            self.space_left_in_page() >= size_of::<T>(),
            "Object of type {} does not fit into a StreamArchiver page.",
            type_name::<T>()
        );

        let ptr = self.current_page.as_ptr().add(self.position_in_current_page);
        self.position_in_current_page += size_of::<T>();
        std::ptr::write_unaligned(ptr as *mut T, object);
        Ok(())
    }

    fn space_left_in_page(&self) -> usize {
        self.page_size - self.position_in_current_page
    }
}


#[derive(Clone)]
pub struct Reader<'a> {
    current_page: &'a Page,
    page_size: usize,
    position_in_current_page: usize,
    current_header: Header,
    allocator: Rc<AllocatedReader<'a>>,
}

impl<'a> Reader<'a> {
    /// PANICS: Panics if `first_page_idx` is `None`.
    pub fn new(first_page_idx: PageIdx, page_size: usize, allocated_reader: AllocatedReader<'a>) -> Self {
        Self {
            current_page: allocated_reader
                .cluster(&first_page_idx.expect("Reader initialized with empty StreamArchiver")),
            page_size,
            position_in_current_page: size_of::<PageIdx>(),
            current_header: Header { len: 0, type_id: TypeId::of::<()>() },
            allocator: Rc::new(allocated_reader),
        }
    }

    /// PANICS: Panics if called after reaching the end of the archive or if the next archived object is not of type
    /// `T`.
    pub fn next<T>(&mut self) -> Vec<T>
    where T: Any {
        // SAFETY: Since `self` was created from a consistent `StreamArchiver`, right after instantiation the object at
        // `self.position_in_current_page` is a `Header`. This method is the only public way to mutate
        // `self.position_in_current_page`, and it ensures that when it returns, the object at
        // `self.position_in_current_page` is the next header.
        // The archive's end is marked by a `Header` with an inaccessible type, so trying to read it will cause a panic.
        unsafe {
            self.read_header();
        }
        assert_eq!(self.current_header.type_id, TypeId::of::<T>());

        let mut result = Vec::new();
        for _ in 0..self.current_header.len {
            // SAFETY: Safe because the header states the next `len` objects are of type `T`.
            let object = unsafe { self.next_object::<T>() };
            result.push(object);
        }
        result
    }

    /// SAFETY: Undefined behavior if the object at `self.position_in_current_page` is not a `Header`.
    unsafe fn read_header(&mut self) {
        self.current_header = self.next_object::<Header>();
    }

    /// SAFETY: Undefined behavior if the object at `self.position_in_current_page` is not of type `T`.
    unsafe fn next_object<T>(&mut self) -> T {
        if self.space_left_in_page() < size_of::<T>() {
            self.next_page();
        }
        assert!(self.space_left_in_page() >= size_of::<T>());

        let ptr = self.current_page.as_ptr().add(self.position_in_current_page);
        self.position_in_current_page += size_of::<T>();
        std::ptr::read_unaligned(ptr as *const T)
    }

    fn space_left_in_page(&self) -> usize {
        self.page_size - self.position_in_current_page
    }

    /// PANICS: Panics if called after reaching the end of the archive.
    fn next_page(&mut self) {
        // SAFETY: Safe because every page begins with the next `PageIdx`.
        let next_page_idx = unsafe { std::ptr::read_unaligned(self.current_page.as_ptr() as *const PageIdx) };
        let next_page_idx = next_page_idx.expect("Attempted to read past StreamArchiver end");
        self.current_page = self.allocator.cluster(&next_page_idx);
        self.position_in_current_page = size_of::<PageIdx>(); // skip next page index
    }
}
