//! Syphon output: publishes the program (and optionally each mapped output) so MadMapper,
//! Resolume, OBS and others can pick Phosphor up as a source. Syphon.framework is loaded at
//! runtime, so Phosphor still runs where it is missing.

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, ProtocolObject};
use objc2::msg_send;
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use objc2_metal::{MTLCommandBuffer, MTLCommandQueue, MTLDevice, MTLTexture};
use std::collections::HashMap;

/// Finds and loads Syphon.framework: inside the app bundle, or next to the repo when run from `target/`.
fn load() -> bool {
    static LOADED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *LOADED.get_or_init(|| {
        let Ok(exe) = std::env::current_exe() else { return false };
        let dir = exe.parent().unwrap_or(std::path::Path::new("."));
        let candidates = [
            dir.join("../Frameworks/Syphon.framework/Syphon"),
            dir.join("../../vendor/Syphon.framework/Syphon"),
            dir.join("../../../vendor/Syphon.framework/Syphon"),
        ];
        for path in candidates {
            if !path.exists() {
                continue;
            }
            let c = std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap();
            // SAFETY: loading a framework image; its Objective-C classes register themselves.
            if !unsafe { libc::dlopen(c.as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL) }.is_null() {
                return AnyClass::get(c"SyphonMetalServer").is_some();
            }
        }
        false
    })
}

struct Server {
    obj: Retained<AnyObject>,
}

impl Server {
    fn new(name: &str, device: &ProtocolObject<dyn MTLDevice>) -> Option<Self> {
        let class = AnyClass::get(c"SyphonMetalServer")?;
        let name = NSString::from_str(name);
        // SAFETY: -[SyphonMetalServer initWithName:device:options:] per SyphonMetalServer.h.
        let obj: Option<Retained<AnyObject>> = unsafe {
            let alloc: objc2::rc::Allocated<AnyObject> = msg_send![class, alloc];
            msg_send![alloc, initWithName: &*name, device: device, options: std::ptr::null::<AnyObject>()]
        };
        obj.map(|obj| Self { obj })
    }

    fn has_clients(&self) -> bool {
        // SAFETY: readonly BOOL property.
        unsafe { msg_send![&*self.obj, hasClients] }
    }

    fn publish(&self, texture: &ProtocolObject<dyn MTLTexture>, buffer: &ProtocolObject<dyn MTLCommandBuffer>, w: u32, h: u32) {
        let region = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w as f64, h as f64));
        // SAFETY: Syphon copies the texture region on `buffer`; we commit it.
        unsafe {
            let _: () = msg_send![&*self.obj, publishFrameTexture: texture, onCommandBuffer: buffer, imageRegion: region, flipped: false];
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // SAFETY: -stop takes no arguments.
        unsafe {
            let _: () = msg_send![&*self.obj, stop];
        }
    }
}

/// Live Syphon servers, keyed by output id (`PROGRAM` for the composition).
pub struct Syphon {
    queue: Option<Retained<ProtocolObject<dyn MTLCommandQueue>>>,
    servers: HashMap<u64, (String, Server)>,
    pub available: bool,
}

pub const PROGRAM: u64 = u64::MAX;

impl Default for Syphon {
    fn default() -> Self {
        Self { queue: None, servers: HashMap::new(), available: load() }
    }
}

impl Syphon {
    /// Publishes each `(id, name, texture)`; servers not listed are stopped.
    pub fn publish(&mut self, device: &wgpu::Device, frames: &[(u64, String, &wgpu::Texture)]) {
        if !self.available {
            return;
        }
        self.servers.retain(|id, _| frames.iter().any(|(f, _, _)| f == id));
        if frames.is_empty() {
            return;
        }
        // SAFETY: only reading the raw Metal handles; wgpu keeps them alive while borrowed.
        let Some(hal_device) = (unsafe { device.as_hal::<wgpu::hal::api::Metal>() }) else { return };
        let raw_device = hal_device.raw_device();
        let queue = self.queue.get_or_insert_with(|| raw_device.newCommandQueue().expect("Metal command queue"));
        let mut buffer = None;
        for (id, name, texture) in frames {
            let label = if *id == PROGRAM { "Phosphor".to_string() } else { format!("Phosphor – {name}") };
            if self.servers.get(id).is_none_or(|(n, _)| *n != label) {
                self.servers.remove(id);
                if let Some(server) = Server::new(&label, raw_device) {
                    self.servers.insert(*id, (label, server));
                }
            }
            let Some((_, server)) = self.servers.get(id) else { continue };
            if !server.has_clients() {
                continue;
            }
            let Some(hal_tex) = (unsafe { texture.as_hal::<wgpu::hal::api::Metal>() }) else { continue };
            let buffer = buffer.get_or_insert_with(|| queue.commandBuffer().expect("Metal command buffer"));
            server.publish(hal_tex.raw_handle(), buffer, texture.width(), texture.height());
        }
        if let Some(buffer) = buffer {
            buffer.commit();
        }
    }

    pub fn server_count(&self) -> usize {
        self.servers.len()
    }

    pub fn clients(&self) -> usize {
        self.servers.values().filter(|(_, s)| s.has_clients()).count()
    }
}
