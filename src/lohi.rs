use std::ops::{AddAssign, SubAssign, Add, Sub};
use std::fmt::Debug;
use std::convert::TryFrom;
use std::marker::PhantomData;
use num::PrimInt;
// TODO integer or primint?


/// Sometimes for space reasons a single 2X-byte wide value is split into two X-byte wide
/// values which are stored non-adjacently on disk. LoHi and LoHiMut provide two abstractions
/// to read and write these values without having to care about their on-disk representation.
pub struct LoHiMut<'a, Full, Half>
where Full: PrimInt + From<Half>,
	  Half: PrimInt + TryFrom<Full>,
	  Half::Error: Debug {
	pub lo: &'a mut Half,
	pub hi: &'a mut Half,
	full: PhantomData<Full>,
}

impl<'a, Full, Half> LoHiMut<'a, Full, Half>
where Full: PrimInt + From<Half>,
	  Half: PrimInt + TryFrom<Full>,
	  Half::Error: Debug {

	const half_bit_count: usize = std::mem::size_of::<Half>() * 8;

	// ideally would be const, but zero is not a const fn
	fn lower_half_mask() -> Full {
		(!Half::zero()).into()
	}

	pub fn new(lo: &'a mut Half, hi: &'a mut Half) -> Self {
		Self { lo, hi, full: PhantomData }
	}

	pub fn get(&self) -> Full {
		let hi: Full = (*self.hi).into();
		let lo: Full = (*self.lo).into();
		(hi << Self::half_bit_count) + lo
	}

	pub fn set(&mut self, value: Full) {
		*self.lo = Half::try_from(value & Self::lower_half_mask()).unwrap();
		*self.hi = Half::try_from(value >> Self::half_bit_count).unwrap();
	}
}

impl<'a, Full, Half> AddAssign<Full> for LoHiMut<'a, Full, Half>
where Full: PrimInt + From<Half>,
	  Half: PrimInt + TryFrom<Full>,
	  Half::Error: Debug {
	fn add_assign(&mut self, other: Full) {
		self.set(self.get() + other);
	}
}

impl<'a, Full, Half> SubAssign<Full> for LoHiMut<'a, Full, Half>
where Full: PrimInt + From<Half>,
	  Half: PrimInt + TryFrom<Full>,
	  Half::Error: Debug {
	fn sub_assign(&mut self, other: Full) {
		self.set(self.get() - other);
	}
}



pub struct LoHi<'a, Full, Half>
where Full: PrimInt + From<Half>,
	  Half: PrimInt + TryFrom<Full>,
	  Half::Error: Debug {
	pub lo: &'a Half,
	pub hi: &'a Half,
	full: PhantomData<Full>,
}

impl<'a, Full, Half> LoHi<'a, Full, Half>
where Full: PrimInt + From<Half>,
	  Half: PrimInt + TryFrom<Full>,
	  Half::Error: Debug {

	const half_bit_count: usize = std::mem::size_of::<Half>() * 8;

	// ideally would be const, but zero is not a const fn
	fn lower_half_mask() -> Full {
		(!Half::zero()).into()
	}

	pub fn new(lo: &'a Half, hi: &'a Half) -> Self {
		Self { lo, hi, full: PhantomData }
	}

	pub fn get(&self) -> Full {
		let hi: Full = (*self.hi).into();
		let lo: Full = (*self.lo).into();
		(hi << Self::half_bit_count) + lo
	}
}

impl<'a, Full, Half> Add<Full> for LoHi<'a, Full, Half>
where Full: PrimInt + From<Half>,
	  Half: PrimInt + TryFrom<Full>,
	  Half::Error: Debug {
	type Output = Full;
	fn add(self, other: Full) -> Self::Output {
		self.get() + other
	}
}

impl<'a, Full, Half> Sub<Full> for LoHi<'a, Full, Half>
where Full: PrimInt + From<Half>,
	  Half: PrimInt + TryFrom<Full>,
	  Half::Error: Debug {
	type Output = Full;
	fn sub(self, other: Full) -> Self::Output {
		self.get() - other
	}
}
