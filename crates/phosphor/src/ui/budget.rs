//! Keeps panels on the kit: outside `ui/`, each file may use raw egui pieces (select-labels, hand-made buttons,
//! checkboxes) only as many times as `raw_budget.txt` says. Moving a panel to the kit lowers its number; nothing new
//! can add one. (Menus' item lists are the exception: `ComboBox` / menu entries go through `ui::pick_item`.)

#[cfg(test)]
mod tests {
    fn raw_count(text: &str) -> usize {
        ["selectable_label(", "selectable_value(", "egui::Button::new(", "ui.checkbox(", "ui.toggle_value("].iter().map(|p| text.matches(p).count()).sum()
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
}
