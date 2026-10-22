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
mod color_match;
pub use color_match::{color_stats, match_values, palette_of};
mod merge;
mod multi_edit;
mod templates;
pub use templates::TEMPLATES;
pub use merge::Merged;
mod remap;
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

choice!(BlendMode, FitMode, CrossGroup, FaderCurve, ChunkBy, PlayDirection, TransitionKind, LaunchTiming, WidgetKind, TriggerMode, VoiceMode, Autopilot, DmxProtocol, PixelOrder, FixtureShape, HeadLayout, HeadMotion, TimecodeSource, QrStyle, RenderRate, ChainMode, TestPattern);
