//! The project/document model. Plain serde data so the same JSON round-trips between
//! native, browser, and (later) cloud-synced accounts.

#[macro_use]
mod choice;
mod composition;
mod control;
mod clip_play;
mod group;
mod layer;
mod options;
mod output;
mod source;
mod surface;

pub use choice::*;
pub use clip_play::*;
pub use composition::*;
pub use control::*;
pub use group::*;
pub use layer::*;
pub use options::*;
pub use output::*;
pub use source::*;
pub use surface::*;

choice!(BlendMode, FitMode, CrossGroup, PlayDirection, TransitionKind, LaunchTiming, WidgetKind, TriggerMode, Autopilot);
