//! More than a number on a built-in OSC address (Resolume-style): `"?"` asks for the value, `"+" 0.1` / `"-"` /
//! `"*"` nudge it, a whole number picks a menu item, a name picks it by name.

use super::*;

/// What a message on a `/phosphor/…` address asks for besides setting a 0..1 value.
#[derive(Clone, Debug, PartialEq)]
pub enum AskKind {
    /// `"?"`: send the current value back.
    Query,
    /// `"+"`, `"-"` or `"*"` with an amount (none = one menu item, or 5% of a slider).
    Nudge(char, Option<f32>),
    /// A whole number: on a menu, the item (from 0).
    Whole(i64),
    /// Words: on a menu, the item with that name.
    Name(String),
}

#[derive(Clone, Debug)]
pub struct OscAsk {
    pub address: String,
    /// The sender's IP (a query's answer goes back there, on the feedback port).
    pub from: String,
    pub kind: AskKind,
}

/// One argument of a message, as far as asks care.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    Text(String),
    Whole(i64),
    Number(f32),
}

/// What a message's arguments ask for; `quiet` = it shouldn't also count as a plain value. None = just a value.
pub fn parse(address: &str, args: &[Arg]) -> Option<(AskKind, bool)> {
    // (A text layer's words are words, whatever they say.)
    if !address.starts_with("/phosphor/") || address.ends_with("/text") {
        return None;
    }
    let number = args.iter().find_map(|a| match a {
        Arg::Number(n) => Some(*n),
        Arg::Whole(i) => Some(*i as f32),
        Arg::Text(_) => None,
    });
    match args.first()? {
        Arg::Text(s) => Some((
            match s.trim() {
                "?" => AskKind::Query,
                "+" => AskKind::Nudge('+', number),
                "-" => AskKind::Nudge('-', number),
                "*" => AskKind::Nudge('*', number),
                _ if number.is_some() => return None,
                name => AskKind::Name(name.to_string()),
            },
            true,
        )),
        Arg::Whole(i) => Some((AskKind::Whole(*i), false)),
        Arg::Number(_) => None,
    }
}

/// A pick-one control's items: each name and the 0..1 value that picks it. None for sliders, switches and presses.
pub fn target_options(comp: &Composition, target: &ControlTarget) -> Option<Vec<(String, f32)>> {
    use phosphor_engine::Choice;
    let of_param = |p: &shaders::Param| (!p.labels.is_empty()).then(|| p.labels.iter().enumerate().map(|(i, l)| (l.to_string(), to_unit(p.min + i as f32, p.min, p.max, false))).collect::<Vec<_>>());
    fn of_choice<C: Choice>() -> Vec<(String, f32)> {
        C::OPTIONS.iter().map(|c| (c.label().to_string(), c.unit())).collect()
    }
    match target {
        ControlTarget::EffectParam { effect, index } => of_param(shaders::effect(&find_effect(comp, *effect)?.effect)?.params.get(*index)?),
        ControlTarget::GeneratorParam { layer, index } => {
            let phosphor_engine::SourceSpec::Generator(g) = &comp.layer(*layer)?.source else { return None };
            of_param(shaders::generator(g)?.params.get(*index)?)
        }
        ControlTarget::LayerDirection(_) => Some(of_choice::<phosphor_engine::PlayDirection>()),
        ControlTarget::LayerBlend(_) => Some(of_choice::<phosphor_engine::BlendMode>()),
        ControlTarget::Surface(id) => {
            let w = comp.surface.widgets.iter().find(|w| w.id == *id)?;
            (w.kind == phosphor_engine::WidgetKind::Choices).then(|| (0..w.choice_count()).map(|i| (w.items.get(i).cloned().unwrap_or_else(|| (i + 1).to_string()), w.choice_value(i))).collect())
        }
        _ => None,
    }
}

/// The built-in addresses of every control, by id and by name: which controls an address (or a pattern) reaches.
pub(super) struct Builtin {
    targets: Vec<(ControlTarget, String, String)>,
}

impl Builtin {
    pub fn new(comp: &Composition) -> Self {
        let names = osc_names(comp);
        Self {
            targets: all_targets(comp)
                .into_iter()
                .map(|(t, _)| {
                    let by_id = default_osc_address(&t);
                    let by_name = named_address(&by_id, &names);
                    (t, by_id, by_name)
                })
                .collect(),
        }
    }

    /// Every control `address` reaches (one, or many for a pattern), each with its by-id address.
    pub fn hits(&self, address: &str) -> Vec<(&ControlTarget, &str)> {
        // Addresses use American spelling since 0.5.6 ("color", "center", "gray" before): an old controller setup still lands.
        // (The address as sent is tried too: a layer really named "greyscale" keeps its by-name address.)
        let raw_lower = address.to_lowercase();
        let address = address.replace("colour", "color").replace("Colour", "Color").replace("centre", "center").replace("Centre", "Center").replace("grey", "gray").replace("Grey", "Gray");
        let lower = address.to_lowercase();
        let pattern = is_osc_pattern(&address);
        let mut out = Vec::new();
        for (t, by_id, by_name) in &self.targets {
            let hit = if pattern {
                osc_match(&lower, &by_id.to_lowercase()) || osc_match(&lower, by_name) || osc_match(&raw_lower, by_name)
            } else {
                *by_id == address || *by_name == lower || *by_name == raw_lower
            };
            if hit {
                out.push((t, by_id.as_str()));
                if !pattern {
                    break;
                }
            }
        }
        out
    }
}

impl ControlHub {
    /// Queries, nudges, menu picks by number and by name that came in on built-in addresses.
    pub(super) fn apply_asks(&mut self, comp: &mut Composition) {
        let asks: Vec<OscAsk> = self.ask_rx.try_iter().collect();
        if asks.is_empty() {
            return;
        }
        let builtin = Builtin::new(comp);
        for ask in asks {
            let address = placed_address(comp, self.selected_layer, &ask.address).unwrap_or_else(|| ask.address.clone());
            let hits: Vec<(ControlTarget, String)> = builtin.hits(&address).into_iter().map(|(t, a)| (t.clone(), a.to_string())).collect();
            let one = hits.len() == 1;
            for (t, by_id) in hits {
                let options = target_options(comp, &t);
                let current = get_target(comp, &t);
                let item_now = |v: f32| options.as_ref().and_then(|o| (0..o.len()).min_by(|a, b| (o[*a].1 - v).abs().total_cmp(&(o[*b].1 - v).abs())));
                let set = match &ask.kind {
                    AskKind::Query => {
                        if let Some(v) = current {
                            let name = item_now(v).and_then(|i| options.as_ref().map(|o| o[i].0.clone()));
                            // Asked by place or by name: the answer comes back on the address that was asked.
                            self.answer(&ask.from, if one { &ask.address } else { &by_id }, v, name);
                        }
                        None
                    }
                    AskKind::Whole(i) => options.as_ref().and_then(|o| o.get(usize::try_from(*i).ok()?).map(|(_, u)| *u)),
                    AskKind::Name(n) => options.as_ref().and_then(|o| o.iter().find(|(l, _)| l.eq_ignore_ascii_case(n)).map(|(_, u)| *u)),
                    AskKind::Nudge(op, amount) => current.and_then(|v| match (&options, item_now(v)) {
                        // A menu steps by whole items (and stops at its ends).
                        (Some(o), Some(i)) if *op != '*' => {
                            let by = amount.unwrap_or(1.0).round() as i64 * if *op == '-' { -1 } else { 1 };
                            Some(o[(i as i64 + by).clamp(0, o.len() as i64 - 1) as usize].1)
                        }
                        _ => Some(
                            match op {
                                '+' => v + amount.unwrap_or(0.05),
                                '-' => v - amount.unwrap_or(0.05),
                                _ => v * amount.unwrap_or(1.0),
                            }
                            .clamp(0.0, 1.0),
                        ),
                    }),
                };
                if let Some(v) = set {
                    self.glides.remove(&format!("{t:?}"));
                    set_target(comp, &t, v, &mut self.triggers);
                }
            }
        }
    }

    /// A query's answer: the value (and a menu item's name) to the asker, on the feedback port.
    fn answer(&mut self, to: &str, address: &str, v: f32, name: Option<String>) {
        #[cfg(not(target_arch = "wasm32"))]
        self.backend.send_osc_answer(to, self.feedback_port, address, v, name);
        #[cfg(target_arch = "wasm32")]
        let _ = (to, address, v, name);
    }

    /// An ask as if it came in over OSC (tests).
    #[cfg(test)]
    pub(crate) fn inject_ask(&self, address: &str, kind: AskKind) {
        let _ = self.ask_tx.send(OscAsk { address: address.into(), from: "127.0.0.1".into(), kind });
    }
}

/// Followed controls whose value changed since `seen` was last filled: what each follower gets now. A control seen
/// for the first time (a project just opened) pushes nothing.
pub fn follow_sends(comp: &Composition, seen: &mut HashMap<ControlTarget, f32>) -> Vec<(ControlTarget, f32)> {
    let mut sends = Vec::new();
    for f in &comp.follows {
        let Some(v) = get_target(comp, &f.source) else { continue };
        if seen.get(&f.source).is_some_and(|old| (old - v).abs() > 1e-4) {
            sends.push((f.to.target.clone(), f.to.value(v)));
        }
    }
    for f in &comp.follows {
        if let Some(v) = get_target(comp, &f.source) {
            seen.insert(f.source.clone(), v);
        }
    }
    sends
}
