//! The three areas (Show, Outputs, Control): each is its own arrangement of panels that you can change freely (drag
//! a tab anywhere, split, stack, pop out into a window). Each area remembers its arrangement, also across restarts
//! (`workspace_docks`); View > Reset Layout puts the current one back to its default (`layout::LAYOUTS`). Every
//! panel's home area is `area_of`; asking for a panel opens its area. The Library (media, effects, presets) is
//! opened and closed as a column on the left (`toggle_library`).

use egui_dock::{DockState, NodeIndex};

use super::PhosphorApp;
use crate::panels::Tab;

/// The Library's panels (opened together by the Library button).
// (Online is inside Media now: its sources' Search libraries. The tab still opens in saved layouts.)
pub(crate) const LIBRARY: &[Tab] = &[Tab::Media, Tab::Inputs, Tab::Library, Tab::Presets];

/// The area a panel belongs to by default (None: any area, e.g. the Library or rarely used panels).
pub(crate) fn area_of(tab: Tab) -> Option<usize> {
    // The Library is a column in any area (open in Show by default), not a panel that belongs to one.
    if LIBRARY.contains(&tab) {
        return None;
    }
    let (_, areas) = super::layout::LAYOUTS.iter().enumerate().fold((0, None), |(i, found), (_, (_, make))| {
        let here = make().iter_all_tabs().any(|(_, t)| *t == tab);
        (i + 1, found.or(here.then_some(i)))
    });
    areas
}

/// Is the Library open in this arrangement?
pub(crate) fn library_open(dock: &DockState<crate::panels::Tab>) -> bool {
    dock.iter_all_tabs().any(|(_, t)| LIBRARY.contains(t))
}

impl PhosphorApp {
    /// Opens the Library as a column on the left of the current area, or closes it.
    pub(crate) fn toggle_library(&mut self) {
        if library_open(&self.dock) {
            for t in LIBRARY {
                if let Some(path) = self.dock.find_tab(t) {
                    self.dock.remove_tab(path);
                }
            }
            self.detached.retain(|t| !LIBRARY.contains(t));
        } else {
            self.dock.main_surface_mut().split_left(NodeIndex::root(), 0.2, LIBRARY.to_vec());
        }
    }

    /// Something asked for a panel: show it where it is; if this area doesn't have it, go to the area whose panel
    /// it is (or add it here when it has no home area, e.g. the Library).
    pub(crate) fn show_tab_anywhere(&mut self, tab: Tab) {
        let here = self.dock.find_tab(&tab).is_some() || self.detached.contains(&tab);
        if !here && let Some(a) = area_of(tab).filter(|a| Some(*a) != self.workspace) {
            self.switch_workspace(a);
        }
        if LIBRARY.contains(&tab) && !library_open(&self.dock) {
            self.toggle_library();
        }
        self.show_tab(tab);
    }

    /// The current area's default arrangement (View > Reset Layout).
    pub(crate) fn reset_area_layout(&mut self) {
        let i = self.workspace.unwrap_or(0);
        if let Some((_, make)) = super::layout::LAYOUTS.get(i) {
            self.dock = make();
            self.detached.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panels_find_their_area() {
        assert_eq!(area_of(Tab::Clips), Some(0));
        assert_eq!(area_of(Tab::Studio), Some(1));
        assert_eq!(area_of(Tab::Surface), Some(2));
        assert_eq!(area_of(Tab::Media), None, "the Library opens in any area");
    }
}
