use std::convert::TryFrom;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::mem::size_of;
use std::ops::{AddAssign, SubAssign};

use num::PrimInt;


/// Sometimes for space/aligment reasons a single value is split into two smaller values which are stored non-adjacently
/// on disk (e.g.: full value 0xBEEF into hi value 0xBE and lo value 0xEF). `LoHi` and `LoHiMut` provide an interface to
/// read and write these values.
pub struct LoHiMut<'a, Full, LoHalf, HiHalf>
where
    Full: PrimInt + From<LoHalf> + From<HiHalf>,
    LoHalf: PrimInt + TryFrom<Full>,
    HiHalf: PrimInt + TryFrom<Full>,
    LoHalf::Error: Debug,
    HiHalf::Error: Debug,
{
    pub lo: &'a mut LoHalf,
    pub hi: &'a mut HiHalf,
    _full: PhantomData<Full>,
}

impl<'a, Full, LoHalf, HiHalf> LoHiMut<'a, Full, LoHalf, HiHalf>
where
    Full: PrimInt + From<LoHalf> + From<HiHalf>,
    LoHalf: PrimInt + TryFrom<Full>,
    HiHalf: PrimInt + TryFrom<Full>,
    LoHalf::Error: Debug,
    HiHalf::Error: Debug,
{
    const LO_HALF_BIT_COUNT: usize = std::mem::size_of::<LoHalf>() * 8;

    // ideally would be const, but `zero` is not a const fn
    fn lo_half_mask() -> Full {
        (!LoHalf::zero()).into()
    }

    /// PANICS: Panics if a `LoHalf` and a `HiHalf` don't fully fit into a `Full`.
    pub fn new(lo: &'a mut LoHalf, hi: &'a mut HiHalf) -> Self {
        // ideally would be a const_assert, but it doesn't work with generics
        assert!(
            size_of::<LoHalf>() + size_of::<HiHalf>() <= size_of::<Full>(),
            "Attempting to create a LoHiMut where a `LoHalf` and a `HiHalf` do not fit into a `Full`."
        );
        Self { lo, hi, _full: PhantomData }
    }

    pub fn get(&self) -> Full {
        let hi: Full = (*self.hi).into();
        let lo: Full = (*self.lo).into();
        (hi << Self::LO_HALF_BIT_COUNT) + lo
    }

    /// PANICS: Panics if `value` does not fit into a `LoHalf` + `HiHalf`.
    pub fn set(&mut self, value: Full) {
        *self.lo = LoHalf::try_from(value & Self::lo_half_mask()).unwrap();
        *self.hi = HiHalf::try_from(value >> Self::LO_HALF_BIT_COUNT).unwrap();
    }
}

impl<'a, Full, LoHalf, HiHalf> AddAssign<Full> for LoHiMut<'a, Full, LoHalf, HiHalf>
where
    Full: PrimInt + From<LoHalf> + From<HiHalf>,
    LoHalf: PrimInt + TryFrom<Full>,
    HiHalf: PrimInt + TryFrom<Full>,
    LoHalf::Error: Debug,
    HiHalf::Error: Debug,
{
    /// PANICS: Panics on overflow
    fn add_assign(&mut self, other: Full) {
        self.set(self.get() + other);
    }
}

impl<'a, Full, LoHalf, HiHalf> SubAssign<Full> for LoHiMut<'a, Full, LoHalf, HiHalf>
where
    Full: PrimInt + From<LoHalf> + From<HiHalf>,
    LoHalf: PrimInt + TryFrom<Full>,
    HiHalf: PrimInt + TryFrom<Full>,
    LoHalf::Error: Debug,
    HiHalf::Error: Debug,
{
    /// PANICS: Panics on underflow
    fn sub_assign(&mut self, other: Full) {
        self.set(self.get() - other);
    }
}


pub struct LoHi<'a, Full, LoHalf, HiHalf>
where
    Full: PrimInt + From<LoHalf> + From<HiHalf>,
    LoHalf: PrimInt + TryFrom<Full>,
    HiHalf: PrimInt + TryFrom<Full>,
{
    pub lo: &'a LoHalf,
    pub hi: &'a HiHalf,
    _full: PhantomData<Full>,
}

impl<'a, Full, LoHalf, HiHalf> LoHi<'a, Full, LoHalf, HiHalf>
where
    Full: PrimInt + From<LoHalf> + From<HiHalf>,
    LoHalf: PrimInt + TryFrom<Full>,
    HiHalf: PrimInt + TryFrom<Full>,
{
    const LO_HALF_BIT_COUNT: usize = size_of::<LoHalf>() * 8;

    /// PANICS: Panics if a `LoHalf` and a `HiHalf` don't fully fit into a `Full`.
    pub fn new(lo: &'a LoHalf, hi: &'a HiHalf) -> Self {
        // ideally would be a const_assert, but it doesn't work with generics
        assert!(
            size_of::<LoHalf>() + size_of::<HiHalf>() <= size_of::<Full>(),
            "Attempting to create a LoHi where a `LoHalf` and a `HiHalf` do not fit into a `Full`."
        );
        Self { lo, hi, _full: PhantomData }
    }

    pub fn get(&self) -> Full {
        let hi: Full = (*self.hi).into();
        let lo: Full = (*self.lo).into();
        (hi << Self::LO_HALF_BIT_COUNT) + lo
    }
}
