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
        Self::OPTIONS[pick(v, Self::OPTIONS.len())]
    }

    /// The 0..1 value that picks this option (the middle of its slice).
    fn unit(self) -> f32 {
        pick_unit(self.index(), Self::OPTIONS.len())
    }
}

/// Which of `n` equal slices of 0..1 a controller value falls in (for lists whose length changes, like pages).
pub fn pick(v: f32, n: usize) -> usize {
    let n = n.max(1);
    ((v.clamp(0.0, 1.0) * n as f32) as usize).min(n - 1)
}

/// The 0..1 value in the middle of slice `i` of `n` (the inverse of `pick`).
pub fn pick_unit(i: usize, n: usize) -> f32 {
    (i as f32 + 0.5) / n.max(1) as f32
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
