//! The project/document model. Plain serde data so the same JSON round-trips between
//! native, browser, and (later) cloud-synced accounts.

#[macro_use]
mod choice;
mod composition;
mod control;
mod clip_play;
mod collections;
mod routing;
mod cues;
mod dmx;
mod group;
mod layer;
mod modulators;
mod options;
mod output;
mod play;
mod quick;
mod source;
mod surface;

pub use choice::*;
pub use clip_play::*;
pub use collections::*;
pub use cues::*;
pub use dmx::*;
pub use composition::*;
pub use control::*;
pub use group::*;
pub use layer::*;
pub use modulators::*;
pub use options::*;
pub use output::*;
pub use play::*;
pub use quick::*;
pub use source::*;
pub use surface::*;

choice!(BlendMode, FitMode, CrossGroup, PlayDirection, TransitionKind, LaunchTiming, WidgetKind, TriggerMode, Autopilot, DmxProtocol, PixelOrder, FixtureShape, HeadLayout, HeadMotion, TimecodeSource, QrStyle, RenderRate, ChainMode);
