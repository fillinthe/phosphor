//! Which graphics card renders (Macs and PCs with more than one): chosen in the Output panel, kept in a small file
//! next to the settings because it's needed before the window (and its settings) exist. Takes effect at the next start.

use std::path::PathBuf;

fn path() -> Option<PathBuf> {
    crate::show::storage_dir().map(|d| d.join("gpu.txt"))
}

/// The card asked for (its name as wgpu reports it), if one was.
pub fn wanted() -> Option<String> {
    std::fs::read_to_string(path()?).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Remembers a card for the next start (None = let the system choose).
pub fn set(name: Option<&str>) -> std::io::Result<()> {
    let Some(p) = path() else { return Ok(()) };
    match name {
        Some(n) => {
            if let Some(dir) = p.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(p, n)
        }
        None => std::fs::remove_file(p).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) }),
    }
}

/// The card named `wanted` if it's there, else the best one: a separate GPU, then the built-in one, then anything. Of
/// the same card's ways in, the steadiest for this system (`backend_rank`).
pub fn pick<'a, T>(adapters: &'a [T], wanted: Option<&str>, info: impl Fn(&T) -> (String, eframe::wgpu::DeviceType, eframe::wgpu::Backend)) -> Option<&'a T> {
    let rank = |t: eframe::wgpu::DeviceType| match t {
        eframe::wgpu::DeviceType::DiscreteGpu => 0,
        eframe::wgpu::DeviceType::IntegratedGpu => 1,
        eframe::wgpu::DeviceType::VirtualGpu => 2,
        // Drawing on the processor comes last, after a real card of a kind the driver doesn't say (an old card
        // through OpenGL): Windows' Basic Render Driver was picked over one for being DirectX 12, at a frame a second
        // (report 2026-11-24).
        eframe::wgpu::DeviceType::Cpu => 4,
        _ => 3,
    };
    let named: Vec<&T> = adapters.iter().filter(|a| wanted.is_some_and(|w| info(a).0 == w)).collect();
    let from: Vec<&T> = if named.is_empty() { adapters.iter().collect() } else { named };
    from.into_iter().min_by_key(|a| {
        let (_, kind, backend) = info(a);
        (rank(kind), backend_rank(backend))
    })
}

/// Windows: DirectX 12 before Vulkan. Lost-device crashes (reports 2026-11-20..22: "Device is lost", "driver
/// implementation is at fault") all came from Vulkan, on laptops with two graphics chips and on AMD integrated graphics;
/// DirectX 12 is what Windows itself and most programs draw with. Elsewhere each system has one way in.
fn backend_rank(b: eframe::wgpu::Backend) -> u8 {
    use eframe::wgpu::Backend as B;
    if cfg!(windows) {
        match b {
            B::Dx12 => 0,
            B::Vulkan => 1,
            _ => 2,
        }
    } else {
        0
    }
}

/// For eframe: picks the saved card (when one was saved); on Windows always (to prefer DirectX 12, see `backend_rank`).
pub fn selector() -> Option<eframe::egui_wgpu::NativeAdapterSelectorMethod> {
    let wanted = wanted();
    if wanted.is_none() && !cfg!(windows) {
        return None;
    }
    Some(std::sync::Arc::new(move |adapters: &[eframe::wgpu::Adapter], _surface: Option<&eframe::wgpu::Surface<'_>>| {
        pick(adapters, wanted.as_deref(), |a| {
            let i = a.get_info();
            (i.name, i.device_type, i.backend)
        })
        .cloned()
        .ok_or_else(|| "no graphics card found".to_string())
    }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_named_card_else_the_strongest() {
        use eframe::wgpu::DeviceType as D;
        use eframe::wgpu::Backend as B;
        let cards = [("Intel UHD", D::IntegratedGpu, B::Vulkan), ("Radeon Pro", D::DiscreteGpu, B::Vulkan), ("Software", D::Cpu, B::Gl)];
        let info = |c: &(&str, D, B)| (c.0.to_string(), c.1, c.2);
        assert_eq!(super::pick(&cards, Some("Intel UHD"), info).map(|c| c.0), Some("Intel UHD"));
        assert_eq!(super::pick(&cards, Some("gone"), info).map(|c| c.0), Some("Radeon Pro"));
        assert_eq!(super::pick(&cards, None, info).map(|c| c.0), Some("Radeon Pro"));
    }

    #[test]
    fn a_real_card_before_drawing_on_the_processor() {
        use eframe::wgpu::{Backend as B, DeviceType as D};
        let cards = [("Microsoft Basic Render Driver", D::Cpu, B::Dx12), ("GeForce 210", D::Other, B::Gl)];
        let info = |c: &(&str, D, B)| (c.0.to_string(), c.1, c.2);
        assert_eq!(super::pick(&cards, None, info).map(|c| c.0), Some("GeForce 210"));
        assert_eq!(super::pick(&cards[..1], None, info).map(|c| c.0), Some("Microsoft Basic Render Driver"), "alone: still used");
    }

    #[test]
    fn windows_draws_with_directx_12_when_the_card_has_it() {
        use eframe::wgpu::{Backend as B, DeviceType as D};
        let cards = [("RTX 4070", D::DiscreteGpu, B::Vulkan), ("RTX 4070", D::DiscreteGpu, B::Dx12), ("Intel UHD", D::IntegratedGpu, B::Dx12)];
        let info = |c: &(&str, D, B)| (c.0.to_string(), c.1, c.2);
        let picked = super::pick(&cards, None, info).unwrap();
        assert_eq!(picked.0, "RTX 4070", "still the stronger card");
        assert_eq!(picked.2, if cfg!(windows) { B::Dx12 } else { B::Vulkan }, "its DirectX 12 way in on Windows");
    }
}
