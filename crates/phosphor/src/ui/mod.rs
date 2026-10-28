//! Reusable UI components shared by every panel. Look here (and extend these) before writing a
//! new widget: see "components first" in the project instructions.

#![allow(unused_imports)]

use eframe::egui::{self, Color32, ComboBox, RichText, Sense, Slider, Stroke, StrokeKind};
use egui_phosphor::regular as icon;
use phosphor_engine::{BlendMode, ControlTarget, EffectState, FitMode, LayerState, MASTER_ID, Mapping, ModSource, Modulation, ParamMod, SourceSpec, TransitionKind, shaders};
use serde::{Deserialize, Serialize};

use crate::app::{State, Trigger};
use crate::theme::{accent, WARN};

pub(crate) mod activity;
pub(crate) mod nodes;
mod binding;
mod budget;
pub(crate) mod eyedropper;
mod kit;
mod learn;
mod level;
mod locks;
pub(crate) mod press;
mod modulation;
mod saves;
mod tool_window;
mod widgets;

pub(crate) use binding::*;
pub(crate) use kit::*;
pub mod notice;
pub(crate) use learn::*;
pub(crate) use level::*;
pub(crate) use locks::*;
pub(crate) use modulation::*;
pub(crate) use saves::*;
pub(crate) use tool_window::*;
pub(crate) use widgets::*;
