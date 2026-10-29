//! Phosphor engine: platform-independent real-time video compositing on wgpu
//! (Metal on macOS/iOS, WebGPU in the browser).

pub mod capture;
pub mod control;
pub mod custom;
mod engine;
mod gpu;
/// Deep colour: whether the graphics card can mix at 16 bits a channel (`gpu::deep_colour_usable`).
pub use gpu::{deep_colour, deep_colour_features, deep_colour_usable, set_deep_colour};
pub mod expr;
pub mod graph;
pub mod ilda;
pub mod laser;
pub mod mapping;
pub mod patch;
pub mod model;
pub mod shaders;

pub use capture::CapturedFrame;
pub use control::ModInputs;
pub use custom::{CompileError, stable_hash};
pub use engine::{CompressedFormat, Engine, ExternalJob, FrameInfo, MASTER_ID, PREVIEW_WIDTH, PreviewItem, PreviewRefresh, THUMBNAIL_WIDTH};
pub use gpu::{FORMAT, Target, f16};
pub use mapping::{Mapping, Surface};
pub use model::*;
pub use wgpu;
