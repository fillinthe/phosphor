//! Syphon output: publishes the program (and optionally each mapped output) so MadMapper,
//! Resolume, OBS and others can pick Phosphor up as a source. Syphon.framework is loaded at
//! runtime, so Phosphor still runs where it is missing.

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, ProtocolObject};
use objc2::msg_send;
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use objc2_metal::{MTLCommandBuffer, MTLCommandQueue, MTLDevice, MTLTexture, MTLTextureType};
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

// ---------------------------------------------------------------------------------------------
// Syphon In: other apps' Syphon outputs as layer sources (zero copy: the shared GPU texture is
// handed to the engine directly).

/// A Syphon source another app is publishing.
#[derive(Clone, Debug, PartialEq)]
pub struct ServerInfo {
    pub app: String,
    pub name: String,
}

impl ServerInfo {
    /// How it's shown and stored in a layer: "App – Name" (or just the app if unnamed).
    pub fn label(&self) -> String {
        if self.name.is_empty() { self.app.clone() } else { format!("{} – {}", self.app, self.name) }
    }
}

/// Reads one of Syphon's exported NSString key constants.
fn key(symbol: &std::ffi::CStr) -> Option<Retained<NSString>> {
    // SAFETY: the symbol is an `NSString * const` exported by Syphon.framework.
    unsafe {
        let p = libc::dlsym(libc::RTLD_DEFAULT, symbol.as_ptr()) as *const *const NSString;
        if p.is_null() || (*p).is_null() {
            return None;
        }
        Retained::retain(*p as *mut NSString)
    }
}

fn dict_string(dict: &AnyObject, k: &NSString) -> String {
    // SAFETY: objectForKey: on an NSDictionary; values for the name/app keys are NSStrings.
    let v: Option<Retained<NSString>> = unsafe { msg_send![dict, objectForKey: k] };
    v.map(|s| s.to_string()).unwrap_or_default()
}

/// All Syphon descriptions currently published (raw NSDictionaries).
fn descriptions() -> Vec<Retained<AnyObject>> {
    if !load() {
        return Vec::new();
    }
    let Some(class) = AnyClass::get(c"SyphonServerDirectory") else { return Vec::new() };
    // SAFETY: +sharedDirectory and -servers per SyphonServerDirectory.h.
    unsafe {
        let dir: Option<Retained<AnyObject>> = msg_send![class, sharedDirectory];
        let Some(dir) = dir else { return Vec::new() };
        let servers: Option<Retained<objc2_foundation::NSArray<AnyObject>>> = msg_send![&*dir, servers];
        servers.map(|a| a.iter().collect()).unwrap_or_default()
    }
}

/// Syphon sources from other apps (Phosphor's own outputs are left out to avoid feedback loops).
pub fn servers() -> Vec<ServerInfo> {
    let (Some(name_key), Some(app_key)) = (key(c"SyphonServerDescriptionNameKey"), key(c"SyphonServerDescriptionAppNameKey")) else { return Vec::new() };
    descriptions()
        .iter()
        .map(|d| ServerInfo { app: dict_string(d, &app_key), name: dict_string(d, &name_key) })
        .filter(|s| !s.app.eq_ignore_ascii_case("phosphor"))
        .collect()
}

/// A connection to one Syphon source, giving its newest frame as a wgpu texture.
pub struct SyphonClient {
    client: Retained<AnyObject>,
}

impl SyphonClient {
    /// Connects to the source labelled `label` (see `ServerInfo::label`).
    pub fn connect(label: &str, device: &wgpu::Device) -> Result<Self, String> {
        if !load() {
            return Err("Syphon isn't available".into());
        }
        let (Some(name_key), Some(app_key)) = (key(c"SyphonServerDescriptionNameKey"), key(c"SyphonServerDescriptionAppNameKey")) else {
            return Err("Syphon isn't available".into());
        };
        let desc = descriptions()
            .into_iter()
            .find(|d| ServerInfo { app: dict_string(d, &app_key), name: dict_string(d, &name_key) }.label() == label)
            .ok_or_else(|| format!("{label}: not publishing right now"))?;
        let class = AnyClass::get(c"SyphonMetalClient").ok_or("Syphon isn't available")?;
        // SAFETY: only reading the raw MTLDevice handle.
        let hal = unsafe { device.as_hal::<wgpu::hal::api::Metal>() }.ok_or("not running on Metal")?;
        let raw_device = hal.raw_device();
        // SAFETY: -[SyphonMetalClient initWithServerDescription:device:options:newFrameHandler:].
        let client: Option<Retained<AnyObject>> = unsafe {
            let alloc: objc2::rc::Allocated<AnyObject> = msg_send![class, alloc];
            msg_send![alloc, initWithServerDescription: &*desc, device: &**raw_device, options: std::ptr::null::<AnyObject>(), newFrameHandler: std::ptr::null::<AnyObject>()]
        };
        client.map(|client| Self { client }).ok_or_else(|| format!("{label}: couldn't connect"))
    }

    /// The newest frame if one arrived since the last call, as a texture the engine can sample.
    pub fn new_frame(&self, device: &wgpu::Device) -> Option<wgpu::Texture> {
        // SAFETY: hasNewFrame / newFrameImage per SyphonMetalClient.h; newFrameImage is +1 retained.
        let tex: Option<Retained<ProtocolObject<dyn MTLTexture>>> = unsafe {
            let fresh: bool = msg_send![&*self.client, hasNewFrame];
            if !fresh {
                return None;
            }
            msg_send![&*self.client, newFrameImage]
        };
        let tex = tex?;
        let (w, h) = (tex.width() as u32, tex.height() as u32);
        let format = match tex.pixelFormat() {
            objc2_metal::MTLPixelFormat::RGBA8Unorm => wgpu::TextureFormat::Rgba8Unorm,
            _ => wgpu::TextureFormat::Bgra8Unorm,
        };
        let size = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
        // SAFETY: wrapping a live, sampleable 2D Metal texture from the same device.
        let hal = unsafe {
            wgpu::hal::metal::Device::texture_from_raw(tex, format, MTLTextureType::Type2D, 1, 1, wgpu::hal::CopyExtent { width: w, height: h, depth: 1 }, None)
        };
        let desc = wgpu::TextureDescriptor {
            label: Some("syphon in"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        // SAFETY: the hal texture matches `desc`.
        Some(unsafe { device.create_texture_from_hal::<wgpu::hal::api::Metal>(hal, &desc, wgpu::TextureUses::RESOURCE) })
    }
}
