//! Pick-one settings: the `Choice` trait and the `choice!` macro that implements it.

/// A setting with a fixed list of options (blend mode, fit, play direction…): one way to list, name and
/// pick them, and to map them to a 0..1 controller value (each option gets an equal slice).
pub trait Choice: Copy + PartialEq + 'static {
    const OPTIONS: &'static [Self];
    fn label(self) -> &'static str;

    fn index(self) -> usize {
        Self::OPTIONS.iter().position(|c| *c == self).unwrap_or(0)
    }

    /// The option a 0..1 controller value picks.
    fn from_unit(v: f32) -> Self {
        let n = Self::OPTIONS.len();
        Self::OPTIONS[((v.clamp(0.0, 1.0) * n as f32) as usize).min(n - 1)]
    }

    /// The 0..1 value that picks this option (the middle of its slice).
    fn unit(self) -> f32 {
        (self.index() as f32 + 0.5) / Self::OPTIONS.len() as f32
    }
}

/// Implements `Choice` from a type's own `ALL` list and `name()`.
macro_rules! choice {
    ($($t:ty),* $(,)?) => {
        $(impl Choice for $t {
            const OPTIONS: &'static [Self] = &<$t>::ALL;
            fn label(self) -> &'static str {
                <$t>::name(self)
            }
        })*
    };
}

#[cfg(test)]
mod choice_tests {
    use crate::model::{BlendMode, Choice, LaunchTiming, PlayDirection};

    /// Every option survives the trip through a controller value, and the slices cover 0..1.
    #[test]
    fn options_round_trip_through_controller_values() {
        fn check<T: Choice + std::fmt::Debug>() {
            for &o in T::OPTIONS {
                assert_eq!(T::from_unit(o.unit()), o, "{o:?}");
            }
            assert_eq!(T::from_unit(0.0), T::OPTIONS[0]);
            assert_eq!(T::from_unit(1.0), *T::OPTIONS.last().unwrap());
        }
        check::<BlendMode>();
        check::<LaunchTiming>();
        check::<PlayDirection>();
    }
}
