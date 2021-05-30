use crate::allocator::{Allocator, AllocatedClusterIdx};
use std::mem::size_of;

type Page = [u8];
type PageIdx = Option<AllocatedClusterIdx>;

pub struct StreamArchiver<'a> {
	head: PageIdx,
	previous_page_idx: PageIdx,
	tail: Vec<u8>,
	current_header: Header, // `current_header.len` encodes how many objects are still expected
	page_size: usize,
	position_in_tail_page: usize,
	allocator: &'a mut Allocator<'a>,
}

#[derive(Copy, Clone)]
struct Header {
	pub len: usize,
	pub size: usize,
}

// options:
// 1) store raw head pointer. method to_read_archiver consumes self and turns it into a reference.
//    problem: we don't necessarily have the mutable borrow on head anymore. so it's possible
//    someone else took it and we're aliasing it.
// 2)
impl<'a> StreamArchiver<'a> {
	pub fn new(allocator: &'a mut Allocator<'a>, page_size: usize) -> Self {
		const MIN_PAGE_PAYLOAD_SIZE: usize = 50;
		assert!(page_size >= size_of::<PageIdx>() + MIN_PAGE_PAYLOAD_SIZE);

		Self {
			head: None,
			tail: vec![0; page_size],
			previous_page_idx: None,
			current_header: Header{ len: 0, size: 0 },
			page_size,
			position_in_tail_page: size_of::<PageIdx>(),
			allocator,
		}
	}

	// TODO make StreamArchiver clusters into used or something??
	pub fn into_reader(mut self) -> Reader<'a> {
		self.write_page();
		Reader::new(self.head, self.page_size, self.allocator)
	}

	pub fn archive<T>(&mut self, objects: Vec<T>) {
		self.add_header(&objects);
		for object in objects {
			self.add_object(object);
		}
	}

	fn write_page(&mut self) {
		let page_idx = Some(self.allocator.allocate(1).start);
		self.allocator.cluster_mut(page_idx.unwrap()).copy_from_slice(&self.tail);

		if self.head.is_none() {
			self.head = page_idx;
		} else { // if head is some, previous_page must be some
			// set the previous page's next page index to the page we just wrote
			let previous_page_idx = self.previous_page_idx.unwrap();
			let previous_page = self.allocator.cluster_mut(previous_page_idx);
			// SAFETY: TODO
			unsafe {
				std::ptr::write_unaligned(previous_page.as_mut_ptr() as *mut PageIdx, page_idx);
			}
		}

		self.previous_page_idx = page_idx;
		self.position_in_tail_page = 0;
		self.tail.fill(0);
		// SAFETY: TODO
		unsafe {
			// this is the last page for now, set the next page index to None
			// this call does not recurse again because there is enough space on the new page
			self.add_stuff::<PageIdx>(None);
		}
	}

	fn add_header<T>(&mut self, objects: &[T]) {
		assert_eq!(self.current_header.len, 0);
		let header = Header { len: objects.len(), size: size_of::<T>() };
		unsafe {
			self.add_stuff(header);
		}
		self.current_header = header;
	}

	fn add_object<T>(&mut self, object: T) {
		assert!(self.current_header.len > 0);
		assert_eq!(self.current_header.size, size_of::<T>());
		unsafe {
			self.add_stuff(object);
		}
		self.current_header.len -= 1;
	}

	/// SAFETY: only safe if consistent with the preceding header. I.e. either
	/// 1) `object` is a header. Then the preceding header must be followed by `preceding_header.len` objects.
	/// 2) `object` has size `preceding_header.size` and fewer than `preceding_header.len`
	/// objects were already added after the header.
	unsafe fn add_stuff<T>(&mut self, object: T) {
		if self.space_left_in_page() < size_of::<T>() {
			self.write_page();
		}
		assert!(self.space_left_in_page() >= size_of::<T>());

		let ptr = self.tail.as_ptr().add(self.position_in_tail_page);
		self.position_in_tail_page += size_of::<T>();
		std::ptr::write_unaligned(ptr as *mut T, object);
	}

	fn space_left_in_page(&self) -> usize {
		self.page_size - self.position_in_tail_page
	}
}


pub struct Reader<'a> {
	current_page: &'a Page,
	page_size: usize,
	position_in_current_page: usize,
	current_header: Header,
	allocator: &'a Allocator<'a>
}

impl<'a> Reader<'a> {
	pub fn new(first_page_idx: PageIdx, page_size: usize, allocator: &'a Allocator) -> Self {
		Self {
			current_page: allocator.cluster(first_page_idx.expect("Reader initialized with empty StreamArchiver")),
			page_size,
			position_in_current_page: size_of::<PageIdx>(),
			current_header: Header { len: 0, size: 0 },
			allocator,
		}
	}

	pub unsafe fn next<T>(&mut self) -> Vec<T> {
		self.read_header();
		assert_eq!(self.current_header.size, size_of::<T>());

		let mut result = Vec::new();
		for _ in 0..self.current_header.len {
			result.push(self.next_stuff::<T>());
		}
		result
	}

	unsafe fn read_header(&mut self) {
		self.current_header = self.next_stuff::<Header>();
	}

	unsafe fn next_stuff<T>(&mut self) -> T {
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

	fn next_page(&mut self) {
		// SAFETY: TODO
		let next_page_idx = unsafe {
			std::ptr::read_unaligned(self.current_page.as_ptr() as *const PageIdx)
		};
		let next_page_idx = next_page_idx.expect("Attempted to read past StreamArchiver end");
		self.current_page = self.allocator.cluster(next_page_idx);
		self.position_in_current_page = size_of::<PageIdx>(); // skip next page index
	}
}
