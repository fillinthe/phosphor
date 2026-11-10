//! Keeps panels on the kit: outside `ui/`, each file may use raw egui pieces (select-labels, hand-made buttons,
//! checkboxes) only as many times as `raw_budget.txt` says. Moving a panel to the kit lowers its number; nothing new
//! can add one. (Menus' item lists are the exception: `ComboBox` / menu entries go through `ui::pick_item`.)

#[cfg(test)]
mod tests {
    fn raw_count(text: &str) -> usize {
        ["selectable_label(", "selectable_value(", "egui::Button::new(", "ui.checkbox(", "ui.toggle_value(", "ui.small_button(", "ui.button(", "egui::ScrollArea::vertical("].iter().map(|p| text.matches(p).count()).sum()
    }

    #[test]
    fn panels_stay_on_the_kit() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let budget: std::collections::HashMap<String, usize> = include_str!("raw_budget.txt")
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .filter_map(|l| l.rsplit_once(' ').map(|(p, n)| (p.to_string(), n.parse().unwrap_or(0))))
            .collect();
        let mut over = Vec::new();
        let mut total = 0;
        let mut stack = vec![src.clone()];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let rel = p.strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/");
                if !rel.ends_with(".rs") || rel.starts_with("ui/") {
                    continue;
                }
                let n = raw_count(&std::fs::read_to_string(&p).unwrap());
                total += n;
                let allowed = budget.get(&rel).copied().unwrap_or(0);
                if n > allowed {
                    over.push(format!("{rel}: {n} raw egui pieces, budget {allowed}"));
                }
            }
        }
        println!("UI_RAW total {total}");
        assert!(over.is_empty(), "use the kit (ui/kit.rs) instead of raw egui pieces:\n{}", over.join("\n"));
    }

    /// Every .rs file under src, (path relative to src, its text).
    fn sources() -> Vec<(String, String)> {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut out = Vec::new();
        let mut stack = vec![src.clone()];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    let rel = p.strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/");
                    out.push((rel, std::fs::read_to_string(&p).unwrap()));
                }
            }
        }
        out.sort();
        out
    }

    /// The lines of `text` that aren't comments or inside its test module (so a rule's own tests don't trip it).
    fn code_lines(text: &str) -> String {
        let marker = concat!("#[cfg", "(test)]");
        let cut = text.find(marker).unwrap_or(text.len());
        text[..cut].lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n")
    }

    /// docs/DESIGN.md, section 7: the patterns no panel may use (each has a kit component instead).
    #[test]
    fn design_rules_hold() {
        let banned: [(&str, &str, &[&str]); 9] = [
            ("egui::Slider", "ui::Level (square bar)", &["ui/"]),
            ("ui.weak(", "ui::hint", &["ui/"]),
            (".radio_value(", "ui::choice_buttons / segmented", &["ui/"]),
            (".toggle_value(", "ui::toggle", &["ui/"]),
            ("ComboBox::from_label(", "ui::dropdown", &["ui/"]),
            ("ComboBox::from_id_salt(", "ui::dropdown (it makes room)", &["ui/"]),
            ("Command::new(\"open\")", "os::open / os::reveal", &["os.rs"]),
            (".dnd_release_payload", "ui::drop_payload", &["ui/widgets.rs"]),
            ("Command::new(", "os::command (no console window on Windows)", &["os.rs", "media/video/tests.rs", "ui/budget.rs"]),
        ];
        let mut bad = Vec::new();
        for (rel, text) in sources() {
            let code = code_lines(&text);
            for (pattern, instead, allowed_in) in &banned {
                if allowed_in.iter().any(|a| rel.starts_with(a)) {
                    continue;
                }
                let n = code.matches(pattern).count();
                if n > 0 {
                    bad.push(format!("{rel}: {n} x {pattern}: use {instead}"));
                }
            }
        }
        assert!(bad.is_empty(), "docs/DESIGN.md, section 7:\n{}", bad.join("\n"));
    }

    /// docs/DESIGN.md, section 3: raw colours outside theme.rs and ui/ only go down (colour_budget.txt).
    #[test]
    fn raw_colours_only_go_down() {
        let budget: std::collections::HashMap<String, usize> = include_str!("colour_budget.txt")
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .filter_map(|l| l.rsplit_once(' ').map(|(p, n)| (p.to_string(), n.parse().unwrap_or(0))))
            .collect();
        let mut over = Vec::new();
        let mut total = 0;
        for (rel, text) in sources() {
            if rel == "theme.rs" || rel.starts_with("ui/") {
                continue;
            }
            let n = ["Color32::from_rgb(", "Color32::from_gray(", "Color32::from_rgba"].iter().map(|p| text.matches(p).count()).sum::<usize>();
            total += n;
            let allowed = budget.get(&rel).copied().unwrap_or(0);
            if n > allowed {
                over.push(format!("{rel}: {n} raw colours, budget {allowed}"));
            } else if n < allowed {
                // A colour moved to theme: the budget goes down with it, so it can't quietly come back.
                over.push(format!("{rel}: {n} raw colours, budget {allowed}: lower its number in ui/colour_budget.txt"));
            }
        }
        println!("UI_RAW_COLOURS total {total}");
        assert!(over.is_empty(), "colours come from theme (accent, Hue, WARN) or the Visuals (docs/DESIGN.md, section 3):\n{}", over.join("\n"));
    }
}
