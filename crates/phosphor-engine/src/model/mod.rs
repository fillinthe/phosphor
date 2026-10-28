//! The project/document model. Plain serde data so the same JSON round-trips between
//! native, browser, and (later) cloud-synced accounts.

#[macro_use]
mod choice;
mod composition;
mod comps;
mod designer;
mod control;
mod control_prefs;
mod clip_play;
mod clip_fx;
mod marks;
pub use marks::*;
pub use designer::{Network, Node as DesignerNode, NodeKind, PortKind};
pub use clip_fx::*;
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
mod timeline;
pub use timeline::{Ease, Event, Key, Marker, MarkerKind, SoundTrack, Step, Timeline, Track};
mod multi_edit;
mod templates;
pub use templates::TEMPLATES;
pub use merge::Merged;
mod remap;
mod source;
mod surface;
mod tracking;
mod live_data;
pub use live_data::*;

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

choice!(BlendMode, FitMode, CrossGroup, FaderCurve, ChunkBy, PlayDirection, TransitionKind, LaunchTiming, WidgetKind, TriggerMode, VoiceMode, Autopilot, DmxProtocol, PixelOrder, FixtureShape, HeadLayout, HeadMotion, TimecodeSource, QrStyle, RenderRate, ChainMode, TestPattern, Ease, MarkerKind, TextMotion);
