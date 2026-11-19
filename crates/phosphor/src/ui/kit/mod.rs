//! Small shared pieces every panel uses the same way, so a field looks like a field and a rename works the same
//! everywhere. (Sliders: `Level`; help text: `hint`; pick-one enums: `choice_buttons` / `choice_combo`.)
//!
//! Buttons by what they do, each with its own look so you can tell them apart at a glance:
//! - `action_button`: makes something happen now (play, trigger, record, Surprise): solid fill, bold.
//! - `toggle`: an on/off state (ON, Loop, GRID, Syphon, mute): a pill with a light, lit when on.
//! - `segmented`: pick one of a few (Forward / Reverse / Ping-pong, Now / Beat / Bar, filters): joined segments,
//!   the picked one raised.
//! - plain `ui.button` = a command (Import…, Rename, Add folder…); `danger_button` = Delete and the like.
//! - `icon_toggle`: an on/off state in a tight row (solo S, "3 fx" bypass, invert arrow): a small square, filled when
//!   on. `eye_toggle`: show / hide (an eye, crossed out in orange when hidden).

mod buttons;
mod fields;
mod layout;
mod words;

pub(crate) use buttons::*;
pub(crate) use fields::*;
pub(crate) use layout::*;
pub(crate) use words::*;
