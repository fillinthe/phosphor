//! Zero-copy hardware video (macOS): a VideoToolbox frame is a CVPixelBuffer backed by an IOSurface. Its two
//! planes (luma R8, chroma RG8) become Metal textures on that IOSurface and then wgpu textures: no CPU copy,
//! no upload. 4:2:0 and 4:2:2 (ProRes 422) in 8 or 10 bits, TV or full range; anything else takes the copying path.

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
    fn CVPixelBufferGetWidthOfPlane(pb: *mut c_void, plane: usize) -> usize;
    fn CVPixelBufferGetHeightOfPlane(pb: *mut c_void, plane: usize) -> usize;
    fn CVPixelBufferGetIOSurface(pb: *mut c_void) -> *mut c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRetain(cf: *const c_void) -> *const c_void;
    fn CFRelease(cf: *const c_void);
}

/// Bi-planar layouts VideoToolbox hands out: (code, 10-bit, full range). 4:2:0 (H.264, HEVC…) and 4:2:2 (ProRes 422:
/// its chroma plane is full height; the shader samples it by 0..1 coordinates, so both draw the same way).
const LAYOUTS: [(&[u8; 4], bool, bool); 8] = [
    (b"420v", false, false),
    (b"420f", false, true),
    (b"x420", true, false),
    (b"xf20", true, true),
    (b"422v", false, false),
    (b"422f", false, true),
    (b"x422", true, false),
    (b"xf22", true, true),
];

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

/// Another reference to the same frame (a retain, no copy): reverse play keeps the end of a forward pass this way.
impl Clone for PixelBuffer {
    fn clone(&self) -> Self {
        // SAFETY: the buffer is alive while `self` holds its reference; this adds one more.
        unsafe { CFRetain(self.0) };
        PixelBuffer(self.0, self.1, self.2)
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
        // SAFETY: valid, retained pixel buffer.
        let surface = unsafe { CVPixelBufferGetIOSurface(self.0) };
        // MTLPixelFormat R8Unorm = 10, RG8Unorm = 30, R16Unorm = 20, RG16Unorm = 60 (10-bit sits in the high bits).
        let (lf, lm, cf, cm) = if self.1 {
            (wgpu::TextureFormat::R16Unorm, 20, wgpu::TextureFormat::Rg16Unorm, 60)
        } else {
            (wgpu::TextureFormat::R8Unorm, 10, wgpu::TextureFormat::Rg8Unorm, 30)
        };
        // SAFETY: the pixel buffer (and so its IOSurface) is retained by `self` while the textures are in use.
        let luma = unsafe { iosurface_texture(device, surface, 0, (lf, lm), (w, h), false) }?;
        // The chroma plane's own size: half height for 4:2:0, full height for 4:2:2.
        // SAFETY: valid, retained pixel buffer with two planes (`retain_nv12` checked its layout).
        let (cw, ch) = unsafe { (CVPixelBufferGetWidthOfPlane(self.0, 1) as u32, CVPixelBufferGetHeightOfPlane(self.0, 1) as u32) };
        let chroma = unsafe { iosurface_texture(device, surface, 1, (cf, cm), (cw.max(1), ch.max(1)), false) }?;
        Some((luma, chroma))
    }
}

/// An IOSurfaceRef as Objective-C sees it (`^{__IOSurface=}`), so message sends type-check in debug builds.
#[repr(C)]
struct IOSurfaceOpaque {
    _private: [u8; 0],
}

// SAFETY: IOSurfaceRef is a pointer to the opaque `struct __IOSurface`.
unsafe impl objc2::encode::RefEncode for IOSurfaceOpaque {
    const ENCODING_REF: objc2::encode::Encoding = objc2::encode::Encoding::Pointer(&objc2::encode::Encoding::Struct("__IOSurface", &[]));
}

/// A wgpu texture on plane `plane` of an IOSurface (no copy): `format` = (wgpu format, MTLPixelFormat number).
/// `render` = it can also be drawn into (RENDER_ATTACHMENT), not only sampled.
///
/// # Safety
/// `surface` must be a valid IOSurfaceRef that outlives the texture, with a plane of that size and format.
pub unsafe fn iosurface_texture(device: &wgpu::Device, surface: *mut c_void, plane: usize, format: (wgpu::TextureFormat, usize), (w, h): (u32, u32), render: bool) -> Option<wgpu::Texture> {
    // SAFETY: only reading the raw MTLDevice of a Metal-backed wgpu device.
    let hal = unsafe { device.as_hal::<wgpu::hal::api::Metal>() }?;
    let raw = hal.raw_device();
    let class = AnyClass::get(c"MTLTextureDescriptor")?;
    // SAFETY: +[MTLTextureDescriptor texture2DDescriptorWithPixelFormat:width:height:mipmapped:],
    // -[MTLDevice newTextureWithDescriptor:iosurface:plane:] (returns +1).
    let tex: Option<Retained<ProtocolObject<dyn MTLTexture>>> = unsafe {
        let desc: *mut AnyObject = msg_send![class, texture2DDescriptorWithPixelFormat: format.1, width: w as usize, height: h as usize, mipmapped: false];
        // MTLTextureUsageShaderRead (1), | RenderTarget (4).
        let _: () = msg_send![desc, setUsage: if render { 5usize } else { 1usize }];
        msg_send![&**raw, newTextureWithDescriptor: desc, iosurface: surface as *mut IOSurfaceOpaque, plane: plane]
    };
    let tex = tex?;
    // SAFETY: a live 2D Metal texture from the same device.
    let hal_tex = unsafe { wgpu::hal::metal::Device::texture_from_raw(tex, format.0, MTLTextureType::Type2D, 1, 1, wgpu::hal::CopyExtent { width: w, height: h, depth: 1 }, None) };
    let usage = if render { wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT } else { wgpu::TextureUsages::TEXTURE_BINDING };
    let desc = wgpu::TextureDescriptor {
        label: Some("iosurface"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: format.0,
        usage,
        view_formats: &[],
    };
    // SAFETY: the hal texture matches `desc`.
    Some(unsafe { device.create_texture_from_hal::<wgpu::hal::api::Metal>(hal_tex, &desc, wgpu::TextureUses::RESOURCE) })
}
