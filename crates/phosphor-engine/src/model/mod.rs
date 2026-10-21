//! The project/document model. Plain serde data so the same JSON round-trips between
//! native, browser, and (later) cloud-synced accounts.

#[macro_use]
mod choice;
mod composition;
mod comps;
mod control;
mod control_prefs;
mod clip_play;
mod collections;
mod routing;
mod sequencer;
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
mod tracking;

pub use choice::*;
pub use clip_play::*;
pub use collections::*;
pub use cues::*;
pub use dmx::*;
pub use composition::*;
pub use comps::*;
pub use control::*;
pub use control_prefs::*;
pub use group::*;
pub use layer::*;
pub use modulators::*;
pub use options::*;
pub use output::*;
pub use play::*;
pub use quick::*;
pub use sequencer::*;
pub use source::*;
pub use surface::*;
pub use tracking::*;

choice!(BlendMode, FitMode, CrossGroup, PlayDirection, TransitionKind, LaunchTiming, WidgetKind, TriggerMode, Autopilot, DmxProtocol, PixelOrder, FixtureShape, HeadLayout, HeadMotion, TimecodeSource, QrStyle, RenderRate, ChainMode, TestPattern);
