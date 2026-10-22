//! Proptest strategies for the types that cannot derive `Arbitrary`.

use proptest::{
    arbitrary::{Arbitrary, any},
    strategy::{BoxedStrategy, Strategy},
};

use crate::{modes::ModeFlags, style::Attrs};

impl Arbitrary for Attrs {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((): ()) -> Self::Strategy {
        any::<u8>().prop_map(Attrs::from_bits_retain).boxed()
    }
}

impl Arbitrary for ModeFlags {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((): ()) -> Self::Strategy {
        any::<u32>().prop_map(ModeFlags::from_bits_retain).boxed()
    }
}

/// 256 palette colours.
pub fn palette_colors() -> BoxedStrategy<[u32; 256]> {
    proptest::collection::vec(any::<u32>(), 256)
        .prop_map(|v| v.try_into().expect("the strategy makes 256 colours"))
        .boxed()
}
