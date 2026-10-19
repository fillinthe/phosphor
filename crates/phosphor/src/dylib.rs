//! Loading a shared library at runtime and looking up its C functions: dlopen / dlsym on the Mac and Linux,
//! LoadLibraryW / GetProcAddress on Windows. For optional runtimes the user installs (NDI).

use std::ffi::{CStr, c_void};
use std::path::Path;

/// A loaded library. It stays loaded for the rest of the process (never closed).
#[derive(Clone, Copy)]
pub struct Library(*mut c_void);

// SAFETY: a module handle is a process-wide token; lookups through it are thread-safe on every platform.
unsafe impl Send for Library {}
unsafe impl Sync for Library {}

impl Library {
    pub fn open(path: &Path) -> Option<Library> {
        #[cfg(unix)]
        {
            let c = std::ffi::CString::new(path.to_string_lossy().as_bytes()).ok()?;
            // SAFETY: loads a library by path; null on failure.
            let h = unsafe { libc::dlopen(c.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
            (!h.is_null()).then_some(Library(h))
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            // SAFETY: loads a library by a NUL-terminated wide path; null on failure.
            let h = unsafe { windows_sys::Win32::System::LibraryLoader::LoadLibraryW(wide.as_ptr()) };
            (!h.is_null()).then_some(Library(h as *mut c_void))
        }
    }

    /// The address of an exported function, if the library has it.
    pub fn sym(&self, name: &CStr) -> Option<*mut c_void> {
        #[cfg(unix)]
        // SAFETY: looks up a symbol in a handle from dlopen.
        let p = unsafe { libc::dlsym(self.0, name.as_ptr()) };
        #[cfg(windows)]
        // SAFETY: looks up an export in a module from LoadLibraryW.
        let p = unsafe { windows_sys::Win32::System::LibraryLoader::GetProcAddress(self.0 as _, name.as_ptr() as *const u8) }
            .map_or(std::ptr::null_mut(), |f| f as *mut c_void);
        (!p.is_null()).then_some(p)
    }
}
