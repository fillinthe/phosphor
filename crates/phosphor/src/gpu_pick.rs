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

/// The card named `wanted` if it's there, else the best one: a separate GPU, then the built-in one, then anything.
pub fn pick<'a, T>(adapters: &'a [T], wanted: &str, info: impl Fn(&T) -> (String, wgpu::DeviceType)) -> Option<&'a T> {
    adapters.iter().find(|a| info(a).0 == wanted).or_else(|| {
        let rank = |t: wgpu::DeviceType| match t {
            wgpu::DeviceType::DiscreteGpu => 0,
            wgpu::DeviceType::IntegratedGpu => 1,
            wgpu::DeviceType::VirtualGpu => 2,
            _ => 3,
        };
        adapters.iter().min_by_key(|a| rank(info(a).1))
    })
}

/// For eframe: picks the saved card (when one was saved).
pub fn selector() -> Option<eframe::egui_wgpu::NativeAdapterSelectorMethod> {
    let wanted = wanted()?;
    Some(std::sync::Arc::new(move |adapters: &[wgpu::Adapter], _surface: Option<&wgpu::Surface<'_>>| {
        pick(adapters, &wanted, |a| {
            let i = a.get_info();
            (i.name, i.device_type)
        })
        .cloned()
        .ok_or_else(|| "no graphics card found".to_string())
    }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_named_card_else_the_strongest() {
        use wgpu::DeviceType as D;
        let cards = [("Intel UHD", D::IntegratedGpu), ("Radeon Pro", D::DiscreteGpu), ("Software", D::Cpu)];
        let info = |c: &(&str, D)| (c.0.to_string(), c.1);
        assert_eq!(super::pick(&cards, "Intel UHD", info).map(|c| c.0), Some("Intel UHD"));
        assert_eq!(super::pick(&cards, "gone", info).map(|c| c.0), Some("Radeon Pro"));
    }
}
