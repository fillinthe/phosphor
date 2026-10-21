//! The OpenGL side of FFGL on the Mac. Plugins draw with OpenGL; Phosphor draws with Metal. They meet on
//! IOSurfaces: each `Shared` picture is one IOSurface seen as a wgpu texture (Metal) and a GL rectangle texture,
//! so nothing is copied through the CPU. Plugins read and draw ordinary GL_TEXTURE_2D textures, so each
//! instance has two of those (`Canvas`), filled from / copied to the shared pictures by `Copier`: one small draw
//! that turns the picture over (GL's pictures start at the bottom, Metal's at the top) and swaps red and blue
//! (GL only takes these surfaces as BGRA; Metal sees the same bytes as RGBA).

use std::ffi::c_void;

use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject};
use objc2_foundation::NSString;

const GL_TEXTURE_2D: u32 = 0x0DE1;
const GL_TEXTURE_RECTANGLE: u32 = 0x84F5;
const GL_RGBA: u32 = 0x1908;
const GL_RGBA8: u32 = 0x8058;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_BGRA: u32 = 0x80E1;
const GL_UNSIGNED_INT_8_8_8_8_REV: u32 = 0x8367;
const GL_VERTEX_SHADER: u32 = 0x8B31;
const GL_FRAGMENT_SHADER: u32 = 0x8B30;
const GL_COMPILE_STATUS: u32 = 0x8B81;
const GL_LINK_STATUS: u32 = 0x8B82;
const GL_TRIANGLES: u32 = 0x0004;
const GL_TEXTURE0: u32 = 0x84C0;
const GL_BLEND: u32 = 0x0BE2;
const GL_FRAMEBUFFER: u32 = 0x8D40;
const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;
const GL_FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;
const GL_COLOR_BUFFER_BIT: u32 = 0x4000;
const GL_LINEAR: i32 = 0x2601;
const GL_TEXTURE_MIN_FILTER: u32 = 0x2801;
const GL_TEXTURE_MAG_FILTER: u32 = 0x2800;
const GL_TEXTURE_WRAP_S: u32 = 0x2802;
const GL_TEXTURE_WRAP_T: u32 = 0x2803;
const GL_CLAMP_TO_EDGE: i32 = 0x812F;

// CGL pixel format attributes.
const PFA_ACCELERATED: i32 = 73;
const PFA_ALLOW_OFFLINE_RENDERERS: i32 = 96;
const PFA_OPENGL_PROFILE: i32 = 99;
const OGL_VERSION_4_CORE: i32 = 0x4100;

#[link(name = "OpenGL", kind = "framework")]
unsafe extern "C" {
    fn CGLChoosePixelFormat(attribs: *const i32, pix: *mut *mut c_void, npix: *mut i32) -> i32;
    fn CGLDestroyPixelFormat(pix: *mut c_void) -> i32;
    fn CGLCreateContext(pix: *mut c_void, share: *mut c_void, ctx: *mut *mut c_void) -> i32;
    fn CGLDestroyContext(ctx: *mut c_void) -> i32;
    fn CGLSetCurrentContext(ctx: *mut c_void) -> i32;
    fn CGLTexImageIOSurface2D(ctx: *mut c_void, target: u32, internal_format: u32, width: i32, height: i32, format: u32, ty: u32, surface: *mut c_void, plane: u32) -> i32;
    fn glGenTextures(n: i32, out: *mut u32);
    fn glDeleteTextures(n: i32, t: *const u32);
    fn glBindTexture(target: u32, t: u32);
    fn glTexParameteri(target: u32, name: u32, v: i32);
    fn glTexImage2D(target: u32, level: i32, internal: i32, w: i32, h: i32, border: i32, format: u32, ty: u32, data: *const c_void);
    fn glGenFramebuffers(n: i32, out: *mut u32);
    fn glDeleteFramebuffers(n: i32, f: *const u32);
    fn glBindFramebuffer(target: u32, f: u32);
    fn glFramebufferTexture2D(target: u32, attachment: u32, textarget: u32, t: u32, level: i32);
    fn glCheckFramebufferStatus(target: u32) -> u32;
    fn glViewport(x: i32, y: i32, w: i32, h: i32);
    fn glClearColor(r: f32, g: f32, b: f32, a: f32);
    fn glClear(mask: u32);
    fn glFlush();
    fn glFenceSync(condition: u32, flags: u32) -> *mut c_void;
    fn glClientWaitSync(sync: *mut c_void, flags: u32, timeout: u64) -> u32;
    fn glDeleteSync(sync: *mut c_void);
    fn glCreateShader(kind: u32) -> u32;
    fn glShaderSource(shader: u32, count: i32, strings: *const *const std::ffi::c_char, lengths: *const i32);
    fn glCompileShader(shader: u32);
    fn glGetShaderiv(shader: u32, name: u32, out: *mut i32);
    fn glDeleteShader(shader: u32);
    fn glCreateProgram() -> u32;
    fn glAttachShader(program: u32, shader: u32);
    fn glLinkProgram(program: u32);
    fn glGetProgramiv(program: u32, name: u32, out: *mut i32);
    fn glUseProgram(program: u32);
    fn glGetUniformLocation(program: u32, name: *const std::ffi::c_char) -> i32;
    fn glUniform1i(location: i32, v: i32);
    fn glUniform2f(location: i32, x: f32, y: f32);
    fn glGenVertexArrays(n: i32, out: *mut u32);
    fn glBindVertexArray(vao: u32);
    fn glDrawArrays(mode: u32, first: i32, count: i32);
    fn glActiveTexture(unit: u32);
    fn glDisable(cap: u32);
}

#[link(name = "IOSurface", kind = "framework")]
unsafe extern "C" {
    fn IOSurfaceCreate(properties: *const c_void) -> *mut c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
}

/// The OpenGL 4.1 context plugins draw in (offscreen, no window).
pub struct Gl {
    ctx: *mut c_void,
}

impl Gl {
    pub fn new() -> Option<Gl> {
        let attribs = [PFA_OPENGL_PROFILE, OGL_VERSION_4_CORE, PFA_ACCELERATED, PFA_ALLOW_OFFLINE_RENDERERS, 0];
        let mut pix = std::ptr::null_mut();
        let mut n = 0;
        let mut ctx = std::ptr::null_mut();
        // SAFETY: a zero-terminated attribute list; out-pointers are valid.
        unsafe {
            if CGLChoosePixelFormat(attribs.as_ptr(), &mut pix, &mut n) != 0 || pix.is_null() {
                return None;
            }
            let err = CGLCreateContext(pix, std::ptr::null_mut(), &mut ctx);
            CGLDestroyPixelFormat(pix);
            if err != 0 || ctx.is_null() {
                return None;
            }
        }
        Some(Gl { ctx })
    }

    /// Makes this the thread's current GL context (before any GL call).
    pub fn make_current(&self) {
        // SAFETY: a live context.
        unsafe { CGLSetCurrentContext(self.ctx) };
    }

    /// Sends what's been drawn to the GPU (without waiting for it).
    pub fn flush(&self) {
        // SAFETY: the context is current (callers draw first).
        unsafe { glFlush() };
    }
}

impl Drop for Gl {
    fn drop(&mut self) {
        // SAFETY: the context is ours; nothing uses it after this.
        unsafe {
            CGLSetCurrentContext(std::ptr::null_mut());
            CGLDestroyContext(self.ctx);
        }
    }
}

/// An RGBA IOSurface of this size (owned: release with CFRelease).
fn iosurface(w: u32, h: u32) -> Option<*mut c_void> {
    let dict_class = AnyClass::get(c"NSMutableDictionary")?;
    let num_class = AnyClass::get(c"NSNumber")?;
    // SAFETY: plain Foundation calls; the dictionary (toll-free bridged to CFDictionary) lives in this scope.
    unsafe {
        let dict: *mut AnyObject = msg_send![dict_class, dictionary];
        let put = |key: &str, v: usize| {
            let n: *mut AnyObject = msg_send![num_class, numberWithUnsignedLong: v];
            let k = NSString::from_str(key);
            let _: () = msg_send![dict, setObject: n, forKey: &*k];
        };
        put("IOSurfaceWidth", w as usize);
        put("IOSurfaceHeight", h as usize);
        put("IOSurfaceBytesPerElement", 4);
        put("IOSurfacePixelFormat", u32::from_be_bytes(*b"RGBA") as usize);
        let s = IOSurfaceCreate(dict as *const c_void);
        (!s.is_null()).then_some(s)
    }
}

/// A plain GL texture with a framebuffer on it.
pub struct Canvas {
    pub tex: u32,
    pub fbo: u32,
    target: u32,
}

impl Canvas {
    fn framebuffer(target: u32, tex: u32) -> Option<u32> {
        let mut fbo = 0;
        // SAFETY: GL calls with a current context and a texture just made.
        unsafe {
            glGenFramebuffers(1, &mut fbo);
            glBindFramebuffer(GL_FRAMEBUFFER, fbo);
            glFramebufferTexture2D(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, target, tex, 0);
            let ok = glCheckFramebufferStatus(GL_FRAMEBUFFER) == GL_FRAMEBUFFER_COMPLETE;
            glBindFramebuffer(GL_FRAMEBUFFER, 0);
            if !ok {
                glDeleteFramebuffers(1, &fbo);
                return None;
            }
        }
        Some(fbo)
    }

    /// An empty GL_TEXTURE_2D (what plugins read and draw into).
    pub fn new(w: u32, h: u32) -> Option<Canvas> {
        let mut tex = 0;
        // SAFETY: GL calls with a current context.
        unsafe {
            glGenTextures(1, &mut tex);
            glBindTexture(GL_TEXTURE_2D, tex);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
            glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA8 as i32, w as i32, h as i32, 0, GL_RGBA, GL_UNSIGNED_BYTE, std::ptr::null());
            glBindTexture(GL_TEXTURE_2D, 0);
        }
        match Canvas::framebuffer(GL_TEXTURE_2D, tex) {
            Some(fbo) => Some(Canvas { tex, fbo, target: GL_TEXTURE_2D }),
            None => {
                // SAFETY: our texture.
                unsafe { glDeleteTextures(1, &tex) };
                None
            }
        }
    }

    pub fn clear(&self, w: u32, h: u32) {
        // SAFETY: GL calls with a current context.
        unsafe {
            glBindFramebuffer(GL_FRAMEBUFFER, self.fbo);
            glViewport(0, 0, w as i32, h as i32);
            glClearColor(0.0, 0.0, 0.0, 0.0);
            glClear(GL_COLOR_BUFFER_BIT);
        }
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        // SAFETY: our GL objects (the host keeps the context current while instances are dropped).
        unsafe {
            glDeleteFramebuffers(1, &self.fbo);
            glDeleteTextures(1, &self.tex);
        }
        let _ = self.target;
    }
}

const VERTEX: &str = "#version 410
out vec2 uv;
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    uv = p;
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
";

/// Reads a shared picture (rectangle texture, pixels) into a plain one.
const FROM_SHARED: &str = "#version 410
uniform sampler2DRect pic;
uniform vec2 size;
in vec2 uv;
out vec4 color;
void main() {
    color = texture(pic, vec2(uv.x, 1.0 - uv.y) * size).bgra;
}
";

/// Writes a plain picture (0..1 coordinates) into a shared one.
const TO_SHARED: &str = "#version 410
uniform sampler2D pic;
in vec2 uv;
out vec4 color;
void main() {
    color = texture(pic, vec2(uv.x, 1.0 - uv.y)).bgra;
}
";

fn program(fragment: &str) -> Option<u32> {
    // SAFETY: GL calls with a current context; the sources are NUL-free strings passed with their lengths.
    unsafe {
        let compile = |kind: u32, src: &str| -> Option<u32> {
            let sh = glCreateShader(kind);
            let ptr = src.as_ptr() as *const std::ffi::c_char;
            let len = src.len() as i32;
            glShaderSource(sh, 1, &ptr, &len);
            glCompileShader(sh);
            let mut ok = 0;
            glGetShaderiv(sh, GL_COMPILE_STATUS, &mut ok);
            (ok != 0).then_some(sh)
        };
        let (v, f) = (compile(GL_VERTEX_SHADER, VERTEX)?, compile(GL_FRAGMENT_SHADER, fragment)?);
        let p = glCreateProgram();
        glAttachShader(p, v);
        glAttachShader(p, f);
        glLinkProgram(p);
        glDeleteShader(v);
        glDeleteShader(f);
        let mut ok = 0;
        glGetProgramiv(p, GL_LINK_STATUS, &mut ok);
        (ok != 0).then_some(p)
    }
}

/// The two copies between shared pictures and plugin pictures (turned over, red and blue swapped).
pub struct Copier {
    from_shared: u32,
    to_shared: u32,
    vao: u32,
}

impl Copier {
    pub fn new() -> Option<Copier> {
        let mut vao = 0;
        // SAFETY: GL call with a current context.
        unsafe { glGenVertexArrays(1, &mut vao) };
        Some(Copier { from_shared: program(FROM_SHARED)?, to_shared: program(TO_SHARED)?, vao })
    }

    fn draw(&self, program: u32, target: u32, from: u32, to_fbo: u32, (w, h): (u32, u32)) {
        // SAFETY: GL calls with a current context, our program, texture and framebuffer. Leaves GL as plugins
        // expect it (nothing bound).
        unsafe {
            glBindFramebuffer(GL_FRAMEBUFFER, to_fbo);
            glViewport(0, 0, w as i32, h as i32);
            glDisable(GL_BLEND);
            glUseProgram(program);
            glActiveTexture(GL_TEXTURE0);
            glBindTexture(target, from);
            glUniform1i(glGetUniformLocation(program, c"pic".as_ptr()), 0);
            glUniform2f(glGetUniformLocation(program, c"size".as_ptr()), w as f32, h as f32);
            glBindVertexArray(self.vao);
            glDrawArrays(GL_TRIANGLES, 0, 3);
            glBindVertexArray(0);
            glBindTexture(target, 0);
            glUseProgram(0);
            glBindFramebuffer(GL_FRAMEBUFFER, 0);
        }
    }

    /// The shared picture `from` into the plugin's input `to`.
    pub fn read(&self, from: &Shared, to: &Canvas) {
        self.draw(self.from_shared, GL_TEXTURE_RECTANGLE, from.canvas.tex, to.fbo, from.size);
    }

    /// The plugin's output `from` into the shared picture `to`.
    pub fn write(&self, from: &Canvas, to: &Shared) {
        self.draw(self.to_shared, GL_TEXTURE_2D, from.tex, to.canvas.fbo, to.size);
    }
}

/// A marker after the GL work queued so far: `done` says when the GPU has got past it.
pub struct Fence(*mut c_void);

impl Fence {
    pub fn new() -> Fence {
        // GL_SYNC_GPU_COMMANDS_COMPLETE.
        // SAFETY: GL call with a current context.
        Fence(unsafe { glFenceSync(0x9117, 0) })
    }

    /// Finished (or failed: don't wait forever on a broken one). Never blocks.
    pub fn done(&self) -> bool {
        // SAFETY: our sync object; a zero timeout only polls. TIMEOUT_EXPIRED = 0x911B.
        self.0.is_null() || unsafe { glClientWaitSync(self.0, 0, 0) } != 0x911B
    }
}

impl Drop for Fence {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: our sync object.
            unsafe { glDeleteSync(self.0) };
        }
    }
}

/// One picture both Metal and GL see: an IOSurface, the wgpu texture on it and a GL rectangle texture on it.
pub struct Shared {
    surface: *mut c_void,
    pub texture: wgpu::Texture,
    pub canvas: Canvas,
    pub size: (u32, u32),
}

impl Shared {
    pub fn new(gl: &Gl, device: &wgpu::Device, w: u32, h: u32) -> Option<Shared> {
        let surface = iosurface(w, h)?;
        // MTLPixelFormatRGBA8Unorm = 70 (the engine's picture format).
        // SAFETY: `surface` is ours and outlives the texture (released in Drop, after the texture).
        let texture = unsafe { crate::media::video::surface::iosurface_texture(device, surface, 0, (phosphor_engine::FORMAT, 70), (w, h), true) };
        let mut tex = 0;
        // SAFETY: GL calls with the context current; the IOSurface matches the size and format given.
        let bound = unsafe {
            glGenTextures(1, &mut tex);
            glBindTexture(GL_TEXTURE_RECTANGLE, tex);
            let err = CGLTexImageIOSurface2D(gl.ctx, GL_TEXTURE_RECTANGLE, GL_RGBA8, w as i32, h as i32, GL_BGRA, GL_UNSIGNED_INT_8_8_8_8_REV, surface, 0);
            glBindTexture(GL_TEXTURE_RECTANGLE, 0);
            err == 0
        };
        let fbo = if bound { Canvas::framebuffer(GL_TEXTURE_RECTANGLE, tex) } else { None };
        match (texture, fbo) {
            (Some(texture), Some(fbo)) => Some(Shared { surface, texture, canvas: Canvas { tex, fbo, target: GL_TEXTURE_RECTANGLE }, size: (w, h) }),
            _ => {
                // SAFETY: our texture and surface.
                unsafe {
                    glDeleteTextures(1, &tex);
                    CFRelease(surface);
                }
                None
            }
        }
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        // The GL texture and framebuffer go with `canvas`; the surface after the wgpu texture lets go of it.
        let surface = self.surface;
        // SAFETY: our retained IOSurface. Metal's texture keeps its own reference while wgpu still uses it.
        unsafe { CFRelease(surface) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gl_context_and_an_rgba_iosurface_texture_work_here() {
        let Some(gl) = Gl::new() else {
            eprintln!("no OpenGL here: skipped");
            return;
        };
        gl.make_current();
        let surface = iosurface(64, 32).expect("IOSurface");
        let mut tex = 0;
        // SAFETY: as in Shared::new.
        let err = unsafe {
            glGenTextures(1, &mut tex);
            glBindTexture(GL_TEXTURE_RECTANGLE, tex);
            CGLTexImageIOSurface2D(gl.ctx, GL_TEXTURE_RECTANGLE, GL_RGBA8, 64, 32, GL_BGRA, GL_UNSIGNED_INT_8_8_8_8_REV, surface, 0)
        };
        assert_eq!(err, 0, "GL takes the IOSurface (as BGRA)");
        assert!(Copier::new().is_some(), "the copy shaders compile");
        assert!(Canvas::framebuffer(GL_TEXTURE_RECTANGLE, tex).is_some());
        assert!(Canvas::new(64, 32).is_some());
        // SAFETY: ours.
        unsafe { CFRelease(surface) };
    }
}
