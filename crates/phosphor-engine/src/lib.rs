//! Phosphor engine: platform-independent real-time video compositing on wgpu
//! (Metal on macOS/iOS, WebGPU in the browser).

pub mod capture;
pub mod control;
pub mod custom;
mod engine;
mod gpu;
pub mod expr;
pub mod mapping;
pub mod patch;
pub mod model;
pub mod shaders;

pub use capture::CapturedFrame;
pub use control::ModInputs;
pub use custom::CompileError;
pub use engine::{CompressedFormat, Engine, ExternalJob, FrameInfo, MASTER_ID, PREVIEW_WIDTH, PreviewItem, THUMBNAIL_WIDTH};
pub use gpu::{FORMAT, Target};
pub use mapping::{Mapping, Surface};
pub use model::*;
pub use wgpu;
