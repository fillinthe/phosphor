//! Zero-copy hardware video (macOS): a VideoToolbox frame is a CVPixelBuffer backed by an IOSurface. Its two
//! planes (luma R8, chroma RG8) become Metal textures on that IOSurface and then wgpu textures: no CPU copy,
//! no upload. 4:2:0 in 8 or 10 bits, TV or full range; anything else takes the copying path.

use std::ffi::c_void;

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, ProtocolObject};
use objc2::msg_send;
use objc2_metal::{MTLTexture, MTLTextureType};

#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    fn CVPixelBufferGetPixelFormatType(pb: *mut c_void) -> u32;
    fn CVPixelBufferGetWidth(pb: *mut c_void) -> usize;
    fn CVPixelBufferGetHeight(pb: *mut c_void) -> usize;
    fn CVPixelBufferGetIOSurface(pb: *mut c_void) -> *mut c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRetain(cf: *const c_void) -> *const c_void;
    fn CFRelease(cf: *const c_void);
}

/// Bi-planar 4:2:0 layouts VideoToolbox hands out: (code, 10-bit, full range).
const LAYOUTS: [(&[u8; 4], bool, bool); 4] = [(b"420v", false, false), (b"420f", false, true), (b"x420", true, false), (b"xf20", true, true)];

/// 10-bit frames need 16-bit textures (set once the GPU is known).
static SIXTEEN_BIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_sixteen_bit(on: bool) {
    SIXTEEN_BIT.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// A retained CVPixelBuffer (released on drop), with its layout. Sent from the decoder thread to the UI thread.
pub struct PixelBuffer(*mut c_void, bool, bool);

// SAFETY: CVPixelBuffers are reference-counted CoreFoundation objects, usable from any thread.
unsafe impl Send for PixelBuffer {}

impl Drop for PixelBuffer {
    fn drop(&mut self) {
        // SAFETY: we hold one retain from `retain`.
        unsafe { CFRelease(self.0) };
    }
}

impl PixelBuffer {
    /// Keeps the frame's pixel buffer if it can be shown without copying (8-bit 4:2:0 video range on an IOSurface).
    ///
    /// # Safety
    /// `pb` is a valid CVPixelBufferRef (a VideoToolbox AVFrame's `data[3]`).
    pub unsafe fn retain_nv12(pb: *mut c_void) -> Option<Self> {
        if pb.is_null() {
            return None;
        }
        let code = unsafe { CVPixelBufferGetPixelFormatType(pb) }.to_be_bytes();
        let (_, ten, full) = *LAYOUTS.iter().find(|(c, _, _)| **c == code)?;
        if (ten && !SIXTEEN_BIT.load(std::sync::atomic::Ordering::Relaxed)) || unsafe { CVPixelBufferGetIOSurface(pb) }.is_null() {
            return None;
        }
        unsafe { CFRetain(pb) };
        Some(Self(pb, ten, full))
    }

    /// Full range (0–255) rather than TV range (16–235).
    pub fn full_range(&self) -> bool {
        self.2
    }

    /// Identity of the memory behind the frame (the decoder cycles through a few of these).
    pub fn surface_id(&self) -> usize {
        // SAFETY: valid, retained pixel buffer.
        unsafe { CVPixelBufferGetIOSurface(self.0) as usize }
    }

    pub fn size(&self) -> (u32, u32) {
        // SAFETY: valid, retained pixel buffer.
        unsafe { (CVPixelBufferGetWidth(self.0) as u32, CVPixelBufferGetHeight(self.0) as u32) }
    }

    /// The luma and chroma planes as wgpu textures sharing the frame's memory.
    pub fn planes(&self, device: &wgpu::Device) -> Option<(wgpu::Texture, wgpu::Texture)> {
        let (w, h) = self.size();
        // SAFETY: only reading the raw MTLDevice of a Metal-backed wgpu device.
        let hal = unsafe { device.as_hal::<wgpu::hal::api::Metal>() }?;
        let raw = hal.raw_device();
        // SAFETY: valid, retained pixel buffer.
        let surface = unsafe { CVPixelBufferGetIOSurface(self.0) };
        let plane = |index: usize, format: wgpu::TextureFormat, mtl_format: usize, pw: u32, ph: u32| -> Option<wgpu::Texture> {
            let class = AnyClass::get(c"MTLTextureDescriptor")?;
            // SAFETY: +[MTLTextureDescriptor texture2DDescriptorWithPixelFormat:width:height:mipmapped:],
            // -[MTLDevice newTextureWithDescriptor:iosurface:plane:] (returns +1).
            let tex: Option<Retained<ProtocolObject<dyn MTLTexture>>> = unsafe {
                let desc: *mut AnyObject = msg_send![class, texture2DDescriptorWithPixelFormat: mtl_format, width: pw as usize, height: ph as usize, mipmapped: false];
                let _: () = msg_send![desc, setUsage: 1usize]; // MTLTextureUsageShaderRead
                msg_send![&**raw, newTextureWithDescriptor: desc, iosurface: surface, plane: index]
            };
            let tex = tex?;
            // SAFETY: a live, sampleable 2D Metal texture from the same device.
            let hal_tex = unsafe {
                wgpu::hal::metal::Device::texture_from_raw(tex, format, MTLTextureType::Type2D, 1, 1, wgpu::hal::CopyExtent { width: pw, height: ph, depth: 1 }, None)
            };
            let desc = wgpu::TextureDescriptor {
                label: Some("video plane"),
                size: wgpu::Extent3d { width: pw, height: ph, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            };
            // SAFETY: the hal texture matches `desc`.
            Some(unsafe { device.create_texture_from_hal::<wgpu::hal::api::Metal>(hal_tex, &desc, wgpu::TextureUses::RESOURCE) })
        };
        // MTLPixelFormat R8Unorm = 10, RG8Unorm = 30, R16Unorm = 20, RG16Unorm = 60 (10-bit sits in the high bits).
        let (lf, lm, cf, cm) = if self.1 {
            (wgpu::TextureFormat::R16Unorm, 20, wgpu::TextureFormat::Rg16Unorm, 60)
        } else {
            (wgpu::TextureFormat::R8Unorm, 10, wgpu::TextureFormat::Rg8Unorm, 30)
        };
        let luma = plane(0, lf, lm, w, h)?;
        let chroma = plane(1, cf, cm, w.div_ceil(2), h.div_ceil(2))?;
        Some((luma, chroma))
    }
}
