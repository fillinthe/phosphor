mod app_icon;
mod epi_integration;
mod event_loop_context;
pub mod run;

/// PHOSPHOR PATCH: see `crate::monitors`.
pub(crate) fn monitors() -> Option<Vec<crate::Monitor>> {
    event_loop_context::with_current_event_loop(|event_loop| {
        let primary = event_loop.primary_monitor();
        event_loop
            .available_monitors()
            .map(|m| {
                let (position, size) = (m.position(), m.size());
                crate::Monitor {
                    name: m.name().unwrap_or_default(),
                    position: [position.x, position.y],
                    size: [size.width, size.height],
                    scale: m.scale_factor(),
                    primary: primary.as_ref() == Some(&m),
                }
            })
            .collect()
    })
}

#[cfg(target_os = "macos")]
pub(crate) mod macos;

/// File storage which can be used by native backends.
#[cfg(feature = "persistence")]
pub mod file_storage;

pub(crate) mod winit_integration;

#[cfg(feature = "glow")]
mod glow_integration;

#[cfg(feature = "wgpu_no_default_features")]
mod wgpu_integration;
