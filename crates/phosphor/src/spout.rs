//! Spout out (Windows): publishes the composition as a Spout 2 sender named "Phosphor" so Resolume,
//! TouchDesigner, OBS (Spout2 plugin), MadMapper and other Windows apps can use it as a live source. The
//! Windows side of `syphon` (one "share with other apps" switch: `Composition::syphon`).
//!
//! Spout 2 is a convention, not a library: a Direct3D 11 texture made shareable, plus named shared memory
//! that receivers read: "SpoutSenderNames" (256-byte name slots, ended by an empty slot), "ActiveSenderName",
//! and one block per sender (named after it) describing the texture (`SharedTextureInfo`). Frames come back
//! from the GPU as RGBA (the same capture NDI and streaming use) and are copied into the shared texture on a
//! worker thread.

use std::ffi::{CString, c_void};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};

use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_RESOURCE_MISC_SHARED, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::IDXGIResource;
use windows::core::Interface;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Memory::{CreateFileMappingA, FILE_MAP_ALL_ACCESS, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, PAGE_READWRITE, UnmapViewOfFile};
use windows_sys::Win32::System::Threading::{CreateMutexA, ReleaseMutex, WaitForSingleObject};

/// Spout's fixed name length and the sender list's default capacity.
const NAME_LEN: usize = 256;
const MAX_SENDERS: usize = 64;
/// DXGI_FORMAT_B8G8R8A8_UNORM: the format every Spout receiver (DirectX or OpenGL) takes.
const FORMAT_BGRA: u32 = 87;

/// Spout's `SharedTextureInfo` (280 bytes), the block receivers read for a sender.
#[repr(C)]
#[derive(Clone, Copy)]
struct SharedTextureInfo {
    share_handle: u32,
    width: u32,
    height: u32,
    format: u32,
    usage: u32,
    /// The sending program's path (shown by receivers).
    description: [u8; 256],
    partner_id: u32,
}

const _: () = assert!(std::mem::size_of::<SharedTextureInfo>() == 280);

/// Named shared memory (+ the "<name>_mutex" Spout guards it with), kept open while we send:
/// the last handle closing makes it disappear.
struct SharedMemory {
    map: HANDLE,
    view: *mut u8,
    size: usize,
    mutex: HANDLE,
}

impl SharedMemory {
    fn open(name: &str, size: usize) -> Option<SharedMemory> {
        let cname = CString::new(name).ok()?;
        let cmutex = CString::new(format!("{name}_mutex")).ok()?;
        // SAFETY: Win32 named file mapping backed by the page file; opens the existing one if another app made it.
        unsafe {
            let map = CreateFileMappingA(INVALID_HANDLE_VALUE, std::ptr::null(), PAGE_READWRITE, 0, size as u32, cname.as_ptr() as *const u8);
            if map.is_null() {
                return None;
            }
            let view: MEMORY_MAPPED_VIEW_ADDRESS = MapViewOfFile(map, FILE_MAP_ALL_ACCESS, 0, 0, size);
            if view.Value.is_null() {
                CloseHandle(map);
                return None;
            }
            let mutex = CreateMutexA(std::ptr::null(), 0, cmutex.as_ptr() as *const u8);
            Some(SharedMemory { map, view: view.Value as *mut u8, size, mutex })
        }
    }

    /// Runs `f` on the bytes while holding Spout's lock for them (skipped if another app holds it too long).
    fn with<R>(&self, f: impl FnOnce(&mut [u8]) -> R) -> Option<R> {
        // SAFETY: the view stays mapped for our lifetime; the mutex serialises access with other Spout apps.
        unsafe {
            let locked = self.mutex.is_null() || WaitForSingleObject(self.mutex, 67) < 0x102;
            if !locked {
                return None;
            }
            let r = f(std::slice::from_raw_parts_mut(self.view, self.size));
            if !self.mutex.is_null() {
                ReleaseMutex(self.mutex);
            }
            Some(r)
        }
    }
}

impl Drop for SharedMemory {
    fn drop(&mut self) {
        // SAFETY: handles and view from `open`, released once.
        unsafe {
            UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS { Value: self.view as *mut c_void });
            CloseHandle(self.map);
            if !self.mutex.is_null() {
                CloseHandle(self.mutex);
            }
        }
    }
}

/// The names in a "SpoutSenderNames" block: 256-byte slots up to the first empty one.
pub(crate) fn read_names(bytes: &[u8]) -> Vec<String> {
    bytes
        .chunks(NAME_LEN)
        .map(|slot| {
            let end = slot.iter().position(|&b| b == 0).unwrap_or(slot.len());
            String::from_utf8_lossy(&slot[..end]).to_string()
        })
        .take_while(|n| !n.is_empty())
        .collect()
}

/// Writes the names back (sorted, as Spout keeps them) with an empty slot after the last.
pub(crate) fn write_names(bytes: &mut [u8], names: &[String]) {
    let mut sorted: Vec<&String> = names.iter().collect();
    sorted.sort();
    sorted.dedup();
    bytes.fill(0);
    for (slot, name) in bytes.chunks_mut(NAME_LEN).zip(sorted) {
        let n = name.len().min(NAME_LEN - 1);
        slot[..n].copy_from_slice(&name.as_bytes()[..n]);
    }
}

/// One live sender: its D3D11 texture, the shared blocks describing it, and the lock around the texture.
struct Sender {
    name: String,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    texture: Option<(ID3D11Texture2D, u32, u32)>,
    names: SharedMemory,
    _active: SharedMemory,
    info: SharedMemory,
    access: HANDLE,
}

fn d3d11_device() -> Option<(ID3D11Device, ID3D11DeviceContext)> {
    let try_type = |kind: D3D_DRIVER_TYPE| {
        let (mut device, mut context) = (None, None);
        // SAFETY: standard D3D11 device creation; outputs are filled on success.
        let ok = unsafe {
            D3D11CreateDevice(None, kind, HMODULE::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut device), None, Some(&mut context))
        }
        .is_ok();
        if ok { device.zip(context) } else { None }
    };
    try_type(D3D_DRIVER_TYPE_HARDWARE).or_else(|| try_type(D3D_DRIVER_TYPE_WARP))
}

impl Sender {
    fn new(name: &str) -> Result<Sender, String> {
        let (device, context) = d3d11_device().ok_or("Direct3D 11 isn't available")?;
        let names = SharedMemory::open("SpoutSenderNames", NAME_LEN * MAX_SENDERS).ok_or("Couldn't open Spout's sender list")?;
        let taken = names.with(|b| read_names(b)).unwrap_or_default();
        if taken.iter().any(|n| n == name) {
            return Err(format!("Another Spout sender is already called \"{name}\""));
        }
        names.with(|b| {
            let mut list = read_names(b);
            list.push(name.to_string());
            write_names(b, &list);
        });
        let active = SharedMemory::open("ActiveSenderName", NAME_LEN).ok_or("Couldn't open Spout's active sender")?;
        active.with(|b| write_names(b, &[name.to_string()]));
        let info = SharedMemory::open(name, std::mem::size_of::<SharedTextureInfo>()).ok_or("Couldn't create the Spout sender")?;
        let cname = CString::new(format!("{name}_SpoutAccessMutex")).map_err(|e| e.to_string())?;
        // SAFETY: named mutex receivers take while they copy the texture.
        let access = unsafe { CreateMutexA(std::ptr::null(), 0, cname.as_ptr() as *const u8) };
        Ok(Sender { name: name.to_string(), device, context, texture: None, names, _active: active, info, access })
    }

    /// (Re)makes the shared texture at a new size and tells receivers about it.
    fn ensure_texture(&mut self, width: u32, height: u32) -> Result<(), String> {
        if self.texture.as_ref().is_some_and(|(_, w, h)| (*w, *h) == (width, height)) {
            return Ok(());
        }
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: D3D11_RESOURCE_MISC_SHARED.0 as u32,
        };
        let mut texture = None;
        // SAFETY: valid description; the texture is owned by this sender.
        unsafe { self.device.CreateTexture2D(&desc, None, Some(&mut texture)) }.map_err(|e| e.to_string())?;
        let texture = texture.ok_or("No texture")?;
        // SAFETY: the texture was created shareable; Spout passes the legacy handle's low 32 bits.
        let handle = unsafe { texture.cast::<IDXGIResource>().and_then(|r| r.GetSharedHandle()) }.map_err(|e| e.to_string())?;
        let mut description = [0u8; 256];
        if let Ok(exe) = std::env::current_exe() {
            let path = exe.to_string_lossy();
            let n = path.len().min(255);
            description[..n].copy_from_slice(&path.as_bytes()[..n]);
        }
        let info = SharedTextureInfo { share_handle: handle.0 as usize as u32, width, height, format: FORMAT_BGRA, usage: 0, description, partner_id: 0 };
        self.info.with(|b| {
            // SAFETY: plain-old-data struct copied into a block of exactly its size.
            let raw = unsafe { std::slice::from_raw_parts(&info as *const SharedTextureInfo as *const u8, std::mem::size_of::<SharedTextureInfo>()) };
            b[..raw.len()].copy_from_slice(raw);
        });
        self.texture = Some((texture, width, height));
        Ok(())
    }

    /// Copies one RGBA frame into the shared texture (as BGRA) under the access lock.
    fn send(&mut self, width: u32, height: u32, mut rgba: Vec<u8>) -> Result<(), String> {
        self.ensure_texture(width, height)?;
        for px in rgba.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        let Some((texture, _, _)) = &self.texture else { return Ok(()) };
        // SAFETY: the mutex is ours or null; the source buffer holds width * height BGRA pixels.
        unsafe {
            let locked = self.access.is_null() || WaitForSingleObject(self.access, 67) < 0x102;
            if !locked {
                return Ok(()); // a receiver is busy with the last frame: skip this one
            }
            self.context.UpdateSubresource(texture, 0, None, rgba.as_ptr() as *const c_void, width * 4, 0);
            self.context.Flush();
            if !self.access.is_null() {
                ReleaseMutex(self.access);
            }
        }
        Ok(())
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        let name = self.name.clone();
        self.names.with(|b| {
            let list: Vec<String> = read_names(b).into_iter().filter(|n| *n != name).collect();
            write_names(b, &list);
        });
        if !self.access.is_null() {
            // SAFETY: our mutex handle, closed once.
            unsafe { CloseHandle(self.access) };
        }
    }
}

struct Running {
    name: String,
    tx: SyncSender<(u32, u32, Vec<u8>)>,
}

/// The Spout output: started, renamed or stopped by `sync`, fed by `push`.
#[derive(Default)]
pub struct Spout {
    running: Option<Running>,
    pacer: crate::pacing::FramePacer,
    pub error: Option<String>,
}

impl Spout {
    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    pub fn name(&self) -> Option<&str> {
        self.running.as_ref().map(|r| r.name.as_str())
    }

    /// Starts or stops the sender to match the setting.
    pub fn sync(&mut self, enabled: bool, name: &str) {
        if !enabled {
            self.running = None;
            return;
        }
        if self.running.as_ref().is_some_and(|r| r.name == name) || self.error.is_some() {
            return;
        }
        let (tx, rx) = sync_channel::<(u32, u32, Vec<u8>)>(2);
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let sender_name = name.to_string();
        let started = std::thread::Builder::new().name("phosphor-spout".into()).spawn(move || {
            // D3D11 objects live and die on this thread.
            let mut sender = match Sender::new(&sender_name) {
                Ok(s) => s,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            let _ = ready_tx.send(Ok(()));
            while let Ok((w, h, rgba)) = rx.recv() {
                if let Err(e) = sender.send(w, h, rgba) {
                    log::warn!("Spout: {e}");
                }
            }
        });
        if let Err(e) = started {
            self.error = Some(e.to_string());
            return;
        }
        match ready_rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(Ok(())) => {
                self.error = None;
                self.running = Some(Running { name: name.to_string(), tx });
            }
            Ok(Err(e)) => self.error = Some(e),
            Err(_) => self.error = Some("Spout didn't start".into()),
        }
    }

    /// Lets the setting be retried after an error (the panel's toggle).
    pub fn clear_error(&mut self) {
        self.error = None;
    }

    /// True when a frame should be captured for Spout now. Spout can't tell whether anyone is receiving,
    /// so this runs at `fps` whenever the sender is on.
    pub fn frame_due(&mut self, now: f64, fps: u32) -> bool {
        self.running() && self.pacer.due(now, fps)
    }

    pub fn push(&mut self, width: u32, height: u32, rgba: Vec<u8>) {
        let Some(r) = &self.running else { return };
        if let Err(TrySendError::Disconnected(_)) = r.tx.try_send((width, height, rgba)) {
            self.running = None;
            self.error = Some("Spout sender stopped".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sender_names_round_trip_sorted_and_terminated() {
        let mut block = vec![0xffu8; NAME_LEN * 4];
        write_names(&mut block, &["Resolume".into(), "Phosphor".into(), "Phosphor".into()]);
        assert_eq!(read_names(&block), vec!["Phosphor".to_string(), "Resolume".to_string()]);
        assert_eq!(block[NAME_LEN * 2], 0, "an empty slot ends the list");
    }

    /// The real thing on Windows: publish a frame and read the shared blocks back the way a receiver does,
    /// then open the texture from a second D3D11 device through the shared handle.
    #[test]
    fn a_sender_is_listed_and_its_texture_opens() {
        let name = format!("Phosphor test {}", std::process::id());
        let mut s = match Sender::new(&name) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("skipped: {e}");
                return;
            }
        };
        s.send(64, 32, vec![200u8; 64 * 32 * 4]).unwrap();
        let listed = s.names.with(|b| read_names(b)).unwrap();
        assert!(listed.contains(&name), "{listed:?}");
        let info = s.info.with(|b| {
            // SAFETY: the block holds one SharedTextureInfo.
            unsafe { std::ptr::read_unaligned(b.as_ptr() as *const SharedTextureInfo) }
        });
        let info = info.unwrap();
        assert_eq!((info.width, info.height, info.format), (64, 32, FORMAT_BGRA));
        assert_ne!(info.share_handle, 0);
        let (other, _) = d3d11_device().unwrap();
        let mut opened: Option<ID3D11Texture2D> = None;
        // SAFETY: opening the legacy shared handle the info block advertises, as Spout receivers do.
        let r = unsafe { other.OpenSharedResource(windows::Win32::Foundation::HANDLE(info.share_handle as usize as *mut c_void), &mut opened) };
        assert!(r.is_ok() && opened.is_some(), "{r:?}");
        drop(s);
        // Gone from the list once stopped (the block may vanish entirely if we were its only user).
        if let Some(names) = SharedMemory::open("SpoutSenderNames", NAME_LEN * MAX_SENDERS) {
            assert!(!names.with(|b| read_names(b)).unwrap().contains(&name));
        }
    }
}
