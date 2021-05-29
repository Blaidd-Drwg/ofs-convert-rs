use crate::allocator::{Allocator, AllocatedClusterIdx};
use std::mem::size_of;

type Page = [u8];
type PageAddress = AllocatedClusterIdx;
const PAGE_ADDRESS_SIZE: usize = size_of::<PageAddress>();

pub struct StreamArchiver<'a> {
	head: Option<PageAddress>,
	previous_page: Option<PageAddress>,
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
		assert!(page_size >= PAGE_ADDRESS_SIZE + MIN_PAGE_PAYLOAD_SIZE);

		Self {
			head: None,
			tail: vec![0; page_size],
			previous_page: None,
			current_header: Header{ len: 0, size: 0 },
			page_size,
			position_in_tail_page: PAGE_ADDRESS_SIZE,
			allocator,
		}
	}

	// pub fn into_reader(self) -> Reader {
		// self.write_page();
		// Reader {
			// current_page: std::slice::from_raw_parts(self.head, self.page_size),
			// page_size: self.page_size,
			// position_in_current_page: size_of::<&Page>(),
		// }
	// }

	pub fn archive<T>(&mut self, objects: Vec<T>) {
		self.add_header(&objects);
		for object in objects {
			self.add_object(object);
		}
	}

	fn write_page(&mut self) {
		let page_idx = self.allocator.allocate(1).start;
		if self.head.is_none() {
			self.head = Some(page_idx);
		} else { // if head is some, previous_page must be some
			let previous_page_idx = self.previous_page.unwrap();
			let previous_page = self.allocator.cluster_mut(previous_page_idx);
			previous_page[..PAGE_ADDRESS_SIZE].copy_from_slice(&page_idx.to_ne_bytes());
		}

		self.allocator.cluster_mut(page_idx).copy_from_slice(&self.tail);
		self.previous_page = Some(page_idx);
		self.tail.fill(0);
		// reserve space for next pointer
		self.position_in_tail_page = PAGE_ADDRESS_SIZE;
	}

	fn add_header<T>(&mut self, objects: &[T]) {
		assert!(self.current_header.len == 0);
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
		let size = size_of::<T>();
		let bytes = std::slice::from_raw_parts(&object as *const T as *const u8, size);
		self.tail[self.position_in_tail_page..self.position_in_tail_page+size].copy_from_slice(bytes);
		self.position_in_tail_page += size;
	}

	fn space_left_in_page(&self) -> usize {
		self.page_size - self.position_in_tail_page
	}
}

pub struct Reader {
	current_page: PageAddress,
	page_size: usize,
	position_in_current_page: usize,
}
