//! Drawing only what's seen at full size. A layer nobody sees this frame (faded out, muted by solo or Mix off, its
//! side of the crossfader down, its group faded out) is drawn at its thumbnail's size: the thumbnail stays live for
//! cueing, and it costs a few hundred pixels across instead of the canvas. And a picture much bigger than the canvas
//! (a 4K clip in an HD show) goes through its effects at the size it's shown at.
//!
//! What can't change size without starting over keeps full size: effects or sources that remember earlier frames
//! (trails, feedback, multi-pass buffers, motion, plug-ins, Remove Background), points, feeds and transitions. A layer
//! that comes back into sight is drawn at full size that same frame.

use super::*;

/// Effects that draw from their own earlier pictures (`LayerResources::history`, sized like the chain).
pub(super) const HISTORY_ENTRIES: [&str; 4] = ["fx_trails", "fx_slitscan", "fx_flow_keep", "fx_pts_particles"];

/// An unseen layer's thumbnail is drawn again every this many frames (10 a second at 60), each layer on its own
/// frame so the work spreads out: at thumbnail size what's left is mostly each pass's own fixed cost.
const QUIET_EVERY: u64 = 6;

/// After this many seconds unseen, a layer's full-size source picture is freed (made again when it's back).
const QUIET_FREE_AFTER: f32 = 2.0;

/// Below this share of its size, a picture goes through its effects smaller (the copy costs less than it saves).
const SHRINK_BELOW: f32 = 0.75;

/// Does this shader draw from earlier frames, or from something sized to the picture it ran at before?
fn remembers(entry: &str) -> bool {
    HISTORY_ENTRIES.contains(&entry) || matches!(entry, "fx_flow" | "fx_cutout" | "fx_cutout_blur") || shaders::pass_plan(entry).is_some() || shaders::is_external(entry) || shaders::points_render(entry)
}

/// Would drawing this chain at another size lose something it carries from frame to frame?
fn chain_remembers<'a>(effects: impl IntoIterator<Item = &'a EffectState>) -> bool {
    effects.into_iter().filter(|e| e.enabled).filter_map(|e| shaders::effect(&e.effect)).any(|d| {
        let sub = shaders::multipass(d.entry);
        shaders::is_points(d.id) || remembers(d.entry) || sub.iter().any(|e| remembers(e))
    })
}

/// Can this layer be drawn at thumbnail size while nobody sees it (nothing in it starts over at another size)?
fn can_draw_small(layer: &LayerState) -> bool {
    let source_ok = match &layer.source {
        SourceSpec::Generator(id) => shaders::generator(id).is_some_and(|d| !shaders::is_external(d.entry) && shaders::pass_plan(d.entry).is_none() && !shaders::is_points(d.id)),
        SourceSpec::Image { .. } | SourceSpec::Movie { .. } | SourceSpec::Live { .. } | SourceSpec::Text(_) => true,
        // A feed copies another picture (often its own, a feedback loop): cheap already, and kept as it is.
        SourceSpec::Feed { .. } | SourceSpec::None => false,
    };
    // (1:1 pixels places the picture by its size in pixels: it would shrink on the canvas, not just get cheaper.)
    source_ok && layer.transition.is_none() && layer.fit != FitMode::Copy && !layer.is_data() && (layer.fx_bypass || !chain_remembers(layer.chain()))
}

/// The size a picture of `frame` pixels goes through its effects at, when it's much bigger than it shows on a
/// `canvas` with `fit` (None: as it is). Its shape is kept, so it sits on the canvas exactly as before.
pub(super) fn chain_size(frame: (u32, u32), canvas: (u32, u32), fit: FitMode) -> Option<(u32, u32)> {
    let (fw, fh) = (frame.0.max(1) as f32, frame.1.max(1) as f32);
    let (rx, ry) = (canvas.0 as f32 / fw, canvas.1 as f32 / fh);
    let shown = match fit {
        FitMode::Fit => rx.min(ry),
        // Stretched: no fewer pixels than the canvas either way.
        FitMode::Fill | FitMode::Stretch => rx.max(ry),
        FitMode::Copy => return None,
    };
    (shown < SHRINK_BELOW).then(|| (((fw * shown).ceil() as u32).max(16), ((fh * shown).ceil() as u32).max(16)))
}

/// `size` scaled by `k` (at least a pixel each way).
pub(super) fn scaled(size: (u32, u32), k: f32) -> (u32, u32) {
    (((size.0 as f32 * k).round() as u32).max(1), ((size.1 as f32 * k).round() as u32).max(1))
}

impl Engine {
    /// Nothing shows this layer's picture at full size this frame: not the mix (`seen` = some of it shows there), a
    /// slice, another layer's feed, a preview window or the Explorer's stage. Its thumbnail still updates.
    pub(super) fn out_of_sight(&self, layer: &LayerState, seen: bool, ctx: &MixCtx) -> bool {
        !seen
            && !ctx.wanted.contains(&layer.id)
            && !ctx.pre_wanted.contains(&layer.id)
            && !ctx.monitors.iter().any(|m| m.0 == layer.id)
            && ctx.canvas.0 > THUMBNAIL_WIDTH
            && can_draw_small(layer)
    }

    /// Is a layer nobody sees (`small`) skipped this frame? Only its thumbnail shows it then, redrawn every
    /// `QUIET_EVERY` frames; the rest of the time nothing is drawn for it. (Not when some of it is mixed into its
    /// group, which is faded out: the group's thumbnail would flicker.) Keeps track of how long it's been unseen.
    pub(super) fn skip_quiet(&mut self, id: u64, small: bool, opacity: f32, time: f32) -> bool {
        let frame = self.frame_index;
        let Some(res) = self.layers.get_mut(&id) else { return false };
        if !small {
            res.quiet_since = None;
            return false;
        }
        let since = *res.quiet_since.get_or_insert(time);
        if time - since > QUIET_FREE_AFTER || time < since {
            res.generator = None;
        }
        opacity <= 0.001 && res.thumbnail.is_some() && (frame + id) % QUIET_EVERY != 0
    }

    /// Does any of group `g`'s picture show this frame (in the mix, on a slice, in a preview window)?
    pub(super) fn group_seen(&self, g: &LayerGroup, ctx: &MixCtx) -> bool {
        (g.enabled && g.in_mix && modulate(g.opacity, 0.0, 1.0, g.opacity_mod.as_ref(), ctx.mods) > 0.001) || ctx.wanted.contains(&g.id) || ctx.monitors.iter().any(|m| m.0 == g.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_runs_its_effects_at_the_size_it_shows() {
        // A 4K clip in an HD show: HD, whatever the fit (the shape is the same).
        assert_eq!(chain_size((3840, 2160), (1920, 1080), FitMode::Fill), Some((1920, 1080)));
        assert_eq!(chain_size((3840, 2160), (1920, 1080), FitMode::Fit), Some((1920, 1080)));
        // Another shape: fitted, the long side meets the canvas; filled or stretched, both sides cover it.
        assert_eq!(chain_size((4000, 4000), (1920, 1080), FitMode::Fit), Some((1080, 1080)));
        assert_eq!(chain_size((4000, 4000), (1920, 1080), FitMode::Fill), Some((1920, 1920)));
        assert_eq!(chain_size((4000, 4000), (1920, 1080), FitMode::Stretch), Some((1920, 1920)));
        // Not much bigger, smaller, or 1:1 pixels: as it is.
        assert_eq!(chain_size((2400, 1350), (1920, 1080), FitMode::Fill), None);
        assert_eq!(chain_size((1280, 720), (1920, 1080), FitMode::Fill), None);
        assert_eq!(chain_size((3840, 2160), (1920, 1080), FitMode::Copy), None);
        // The shape kept: the fit puts it in the same place.
        let small = chain_size((3000, 2000), (1920, 1080), FitMode::Fill).unwrap();
        let (a, b) = (fit::fit_transform((3000, 2000), (1920, 1080), FitMode::Fill), fit::fit_transform(small, (1920, 1080), FitMode::Fill));
        assert!(a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3), "{a:?} vs {b:?}");
    }

    #[test]
    fn layers_that_remember_frames_keep_their_size() {
        let mut comp = Composition::default();
        let mut layer = LayerState { id: 1, source: SourceSpec::Generator("plasma".into()), ..Default::default() };
        assert!(can_draw_small(&layer));
        layer.effects.push(comp.new_effect("kaleidoscope").unwrap());
        assert!(can_draw_small(&layer), "an effect without memory");
        layer.effects.push(comp.new_effect("trails").unwrap());
        assert!(!can_draw_small(&layer), "trails start over at another size");
        layer.effects[1].enabled = false;
        assert!(can_draw_small(&layer), "a switched-off one doesn't count");
        layer.fit = FitMode::Copy;
        assert!(!can_draw_small(&layer), "1:1 pixels");
        layer.fit = FitMode::Fill;
        layer.source = SourceSpec::Feed { id: crate::MASTER_ID, before_fx: false, name: String::new() };
        assert!(!can_draw_small(&layer), "feeds");
    }
}
