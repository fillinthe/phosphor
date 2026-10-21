//! Audience play: people scan a QR code and take turns at a few controls for a short while each.
//! These are the show's settings (saved with the project); the turn-taking itself lives in the app.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudiencePlay {
    pub enabled: bool,
    /// How long one person has the controls.
    pub turn_secs: u32,
    /// After a turn, how long that phone sits out before it can play again (0 = no rest: with nobody
    /// waiting, a player keeps the controls).
    pub rest_secs: u32,
    /// How many people play at once (each gets a Surface page when no page is chosen).
    pub players: u32,
    /// Control Surface page guests get (None = every page).
    pub surface_page: Option<usize>,
    /// Each place's own page (player 1, 2…), before `surface_page` (None = the usual rule).
    pub place_pages: Vec<Option<usize>>,
    /// Guests may trigger clip columns.
    pub columns: bool,
    /// Guests get a Surprise button.
    pub surprise: bool,
    /// Heading on the guest page.
    pub title: String,
    /// Keep a "Now playing: …" text layer on the output with the players' names.
    pub show_names: bool,
    /// How the code looks on the output layer.
    pub qr_style: QrStyle,
    /// Title and address under the code.
    pub qr_caption: bool,
}

/// The code's colours on the output: a card, or just the modules over the visuals.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum QrStyle {
    #[default]
    BlackOnWhite,
    WhiteOnBlack,
    /// Black modules, no background.
    Black,
    /// White modules, no background.
    White,
}

impl QrStyle {
    pub const ALL: [QrStyle; 4] = [QrStyle::BlackOnWhite, QrStyle::WhiteOnBlack, QrStyle::Black, QrStyle::White];

    pub fn name(self) -> &'static str {
        match self {
            QrStyle::BlackOnWhite => "Black on white",
            QrStyle::WhiteOnBlack => "White on black",
            QrStyle::Black => "Black, no background",
            QrStyle::White => "White, no background",
        }
    }

    /// (modules, background); background None = transparent.
    pub fn colors(self) -> ([u8; 4], Option<[u8; 4]>) {
        match self {
            QrStyle::BlackOnWhite => ([0, 0, 0, 255], Some([255, 255, 255, 255])),
            QrStyle::WhiteOnBlack => ([255, 255, 255, 255], Some([0, 0, 0, 255])),
            QrStyle::Black => ([0, 0, 0, 255], None),
            QrStyle::White => ([255, 255, 255, 255], None),
        }
    }
}

impl Default for AudiencePlay {
    fn default() -> Self {
        Self { enabled: false, turn_secs: 60, rest_secs: 0, players: 1, surface_page: None, place_pages: Vec::new(), columns: true, surprise: true, title: "Take the controls".into(), show_names: false, qr_style: QrStyle::default(), qr_caption: true }
    }
}
