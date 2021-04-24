use std::ops::{AddAssign, SubAssign};
use std::convert::TryFrom;


pub struct LoHi64<'a> {
	pub lo: &'a mut u32,
	pub hi: &'a mut u32,
}

impl<'a> LoHi64<'a> {
	pub fn get(&self) -> u64 { 
		(u64::from(*self.hi) << 32) + u64::from(*self.lo)
	}

	pub fn set(&mut self, value: u64) {
		*self.lo = u32::try_from(value & 0xFFFFFFFF).unwrap();
		*self.hi = u32::try_from(value >> 32).unwrap();
	}
}

impl<'a> AddAssign<u64> for LoHi64<'a> {
	fn add_assign(&mut self, other: u64) {
		self.set(self.get() + other);
	}
}

impl<'a> SubAssign<u64> for LoHi64<'a> {
	fn sub_assign(&mut self, other: u64) {
		self.set(self.get() - other);
	}
}

pub struct LoHi32<'a> {
	pub lo: &'a mut u16,
	pub hi: &'a mut u16,
}

impl<'a> LoHi32<'a> {
	pub fn get(&self) -> u32 { 
		(u32::from(*self.hi) << 16) + u32::from(*self.lo)
	}

	pub fn set(&mut self, value: u32) {
		*self.lo = u16::try_from(value & 0xFFFF).unwrap();
		*self.hi = u16::try_from(value >> 16).unwrap();
	}
}

impl<'a> AddAssign<u32> for LoHi32<'a> {
	fn add_assign(&mut self, other: u32) {
		self.set(self.get() + other);
	}
}

impl<'a> SubAssign<u32> for LoHi32<'a> {
	fn sub_assign(&mut self, other: u32) {
		self.set(self.get() - other);
	}
}
