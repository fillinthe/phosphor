//! Audience play: people scan a QR code and take turns at a few controls for a short while each.
//! These are the show's settings (saved with the project); the turn-taking itself lives in the app.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudiencePlay {
    pub enabled: bool,
    /// How long one person has the controls.
    pub turn_secs: u32,
    /// Control Surface page guests get (None = every page).
    pub surface_page: Option<usize>,
    /// Guests may trigger clip columns.
    pub columns: bool,
    /// Guests get a Surprise button.
    pub surprise: bool,
    /// Heading on the guest page.
    pub title: String,
}

impl Default for AudiencePlay {
    fn default() -> Self {
        Self { enabled: false, turn_secs: 60, surface_page: None, columns: true, surprise: true, title: "Take the controls".into() }
    }
}
