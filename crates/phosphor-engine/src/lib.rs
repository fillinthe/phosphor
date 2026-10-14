//! Phosphor engine: platform-independent real-time video compositing on wgpu
//! (Metal on macOS/iOS, WebGPU in the browser).

pub mod capture;
pub mod control;
mod engine;
mod gpu;
pub mod mapping;
pub mod model;
pub mod shaders;

pub use capture::CapturedFrame;
pub use control::ModInputs;
pub use engine::{Engine, FrameInfo, MASTER_ID, THUMBNAIL_WIDTH};
pub use gpu::{FORMAT, Target};
pub use mapping::{Mapping, Surface};
pub use model::*;
pub use wgpu;
