//! Eyedropper: a colour button waits (`arm`), a click on the preview asks for the colour there (`want`), the app
//! reads that pixel once (`take_request` → `deliver`) and the waiting button takes it (`take_result`).
use eframe::egui;

fn armed_key() -> egui::Id {
    egui::Id::new("eyedropper_armed")
}
fn request_key() -> egui::Id {
    egui::Id::new("eyedropper_request")
}
fn result_key() -> egui::Id {
    egui::Id::new("eyedropper_result")
}

/// The colour button `id` waits for a pick (None = stop waiting).
pub(crate) fn arm(ctx: &egui::Context, id: Option<egui::Id>) {
    ctx.data_mut(|d| d.insert_temp(armed_key(), id));
}

/// The colour button waiting for a pick, if any.
pub(crate) fn armed(ctx: &egui::Context) -> Option<egui::Id> {
    ctx.data(|d| d.get_temp::<Option<egui::Id>>(armed_key())).flatten()
}

/// The picture was clicked at `uv` (0..1 of the program picture).
pub(crate) fn want(ctx: &egui::Context, uv: [f32; 2]) {
    ctx.data_mut(|d| d.insert_temp(request_key(), Some(uv)));
}

pub(crate) fn take_request(ctx: &egui::Context) -> Option<[f32; 2]> {
    ctx.data_mut(|d| d.remove_temp::<Option<[f32; 2]>>(request_key())).flatten()
}

/// The colour read at the requested point goes to the waiting button.
pub(crate) fn deliver(ctx: &egui::Context, rgb: [f32; 3]) {
    if let Some(id) = armed(ctx) {
        ctx.data_mut(|d| d.insert_temp(result_key(), Some((id, rgb))));
        arm(ctx, None);
    }
}

/// A picked colour for the button `id`, once.
pub(crate) fn take_result(ctx: &egui::Context, id: egui::Id) -> Option<[f32; 3]> {
    let r = ctx.data(|d| d.get_temp::<Option<(egui::Id, [f32; 3])>>(result_key())).flatten()?;
    if r.0 != id {
        return None;
    }
    ctx.data_mut(|d| d.remove_temp::<Option<(egui::Id, [f32; 3])>>(result_key()));
    Some(r.1)
}

/// The colour at `uv` in an RGBA8 picture `w` × `h`.
pub(crate) fn pixel_at(w: u32, h: u32, px: &[u8], uv: [f32; 2]) -> Option<[f32; 3]> {
    if w == 0 || h == 0 {
        return None;
    }
    let x = ((uv[0].clamp(0.0, 1.0) * w as f32) as u32).min(w - 1);
    let y = ((uv[1].clamp(0.0, 1.0) * h as f32) as u32).min(h - 1);
    let i = ((y * w + x) * 4) as usize;
    let p = px.get(i..i + 3)?;
    Some([p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_go_to_the_waiting_button() {
        let ctx = egui::Context::default();
        let (a, b) = (egui::Id::new("a"), egui::Id::new("b"));
        arm(&ctx, Some(a));
        want(&ctx, [0.75, 0.25]);
        let uv = take_request(&ctx).unwrap();
        // A 2×2 picture: top right is red.
        let px = [0, 0, 0, 255, 255, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255];
        deliver(&ctx, pixel_at(2, 2, &px, uv).unwrap());
        assert_eq!(take_result(&ctx, b), None);
        assert_eq!(take_result(&ctx, a), Some([1.0, 0.0, 0.0]));
        assert_eq!(take_result(&ctx, a), None, "once");
        assert_eq!(armed(&ctx), None);
    }
}
