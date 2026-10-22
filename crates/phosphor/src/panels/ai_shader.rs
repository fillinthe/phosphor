//! "Describe it…" (Shader Editor): say what the shader should look like and the AI writes it (`crate::ai_shader`).
//! The new shader opens in the editor like any other; "Change the open shader" rewrites the one you're editing.

use crate::ai_shader::{Job, Status};

use super::*;

/// What it makes.
#[derive(Clone, Copy, PartialEq)]
enum Make {
    Effect,
    Generator,
    /// Rewrites the shader open in the editor.
    Change,
}

/// The window's state (in `State::ai_shader`).
#[derive(Default)]
pub struct AiShader {
    pub open: bool,
    description: String,
    make: Option<Make>,
    /// The saved key, looked up once when the window opens (`None` = not looked yet).
    key: Option<Option<String>>,
    /// Typed into the key field.
    new_key: String,
    key_error: Option<String>,
    job: Option<Job>,
    /// Which shader a Change job rewrites (its slug), and what the AI said about the last one.
    changing: Option<String>,
    note: String,
    /// How the last job ended (shown in the window; bench AI line).
    last: Option<Status>,
}

impl AiShader {
    /// Bench `--quick ai` (PHOSPHOR_AI_URL, PHOSPHOR_AI_PROMPT): asks for a generator like the window does.
    pub fn bench_ask(&mut self, description: &str) {
        let key = crate::ai_shader::load_key().unwrap_or_else(|| "sk-ant-bench-0000000000000000".into());
        self.description = description.to_string();
        self.open = true;
        self.job = Some(Job::start(key, description.to_string(), true, None, None));
    }

    /// What the last job did (bench AI line).
    pub fn status(&self) -> Option<&Status> {
        self.job.as_ref().map(|j| &j.status).or(self.last.as_ref())
    }
}

/// The window, while open; also brings in a finished shader (call every frame).
pub(crate) fn ai_shader_window(ctx: &egui::Context, st: &mut State) {
    if let Some(job) = st.ai_shader.job.as_mut()
        && let Status::Done { source, note } = job.poll().clone()
    {
        st.ai_shader.last = st.ai_shader.job.take().map(|j| j.status);
        st.ai_shader.note = note;
        let target = st.ai_shader.changing.take().and_then(|slug| st.editor.shaders.iter().position(|s| s.slug == slug));
        match target {
            Some(i) => crate::explorer::replace_source(st, i, source),
            None => crate::explorer::new_shader(st, source),
        }
        st.notice = Some("The AI's shader is in the Shader Editor".into());
    }
    let open_shader = st.editor.current.and_then(|i| st.editor.shaders.get(i)).map(|s| (s.slug.clone(), s.source.clone()));
    let a = &mut st.ai_shader;
    if !a.open {
        return;
    }
    let key = a.key.get_or_insert_with(crate::ai_shader::load_key).clone();
    let mut open = true;
    egui::Window::new(label(icon::MAGIC_WAND, "Describe a shader")).open(&mut open).default_width(480.0).show(ctx, |ui| {
        hint(ui, "Say what it should look like and the AI writes it: a new effect or source, or a change to the shader you have open. It opens in the Shader Editor, every slider ready to map.");
        let mut options = vec![(Make::Effect, "New effect".to_string()), (Make::Generator, "New source".to_string())];
        if open_shader.is_some() {
            options.push((Make::Change, "Change the open shader".to_string()));
        }
        let mut make = a.make.filter(|m| *m != Make::Change || open_shader.is_some()).unwrap_or(Make::Effect);
        segmented(ui, &mut make, &options, |_, _, _| {});
        a.make = Some(make);
        let example = match make {
            Make::Effect => "e.g. a VHS look: wobbly lines, colour bleeding sideways, a little noise",
            Make::Generator => "e.g. slow neon tunnels that pulse, purple and teal",
            Make::Change => "e.g. make it react faster and add a hue shift slider",
        };
        ui.add(egui::TextEdit::multiline(&mut a.description).hint_text(example).desired_rows(3).desired_width(f32::INFINITY));
        let busy = a.job.as_ref().is_some_and(|j| matches!(j.status, Status::Working(_)));
        ui.horizontal(|ui| {
            let ready = key.is_some() && !busy && !a.description.trim().is_empty();
            if ui.add_enabled_ui(ready, |ui| action_button(ui, Act::Go, format!("{} Write it", icon::MAGIC_WAND))).inner.clicked()
                && let Some(k) = key.clone()
            {
                let change = (make == Make::Change).then(|| open_shader.clone()).flatten();
                a.changing = change.as_ref().map(|(slug, _)| slug.clone());
                a.note.clear();
                a.job = Some(Job::start(k, a.description.trim().to_string(), make == Make::Generator, change.map(|(_, src)| src), Some(ui.ctx().clone())));
            }
            if busy {
                ui.spinner();
            }
        });
        match a.job.as_ref().map(|j| &j.status) {
            Some(Status::Working(s)) => {
                hint(ui, s.as_str());
            }
            Some(Status::Failed(e)) => {
                ui.colored_label(WARN, e.as_str());
            }
            _ if !a.note.is_empty() => {
                ui.colored_label(accent(), format!("{} {}", icon::CHECK, a.note));
            }
            _ => {}
        }
        ui.separator();
        match &key {
            Some(_) => {
                ui.horizontal(|ui| {
                    hint(ui, if std::env::var_os("ANTHROPIC_API_KEY").is_some() { "Using the key in ANTHROPIC_API_KEY." } else { "Your Anthropic API key is saved (Keychain)." });
                    if std::env::var_os("ANTHROPIC_API_KEY").is_none() && ui.small_button("Forget it").clicked() {
                        a.key_error = crate::ai_shader::store_key("").err();
                        a.key = None;
                    }
                });
            }
            None => {
                hint(ui, "Needs your own Anthropic API key (console.anthropic.com › API keys). Each shader costs a few cents on your account. The key stays on this Mac, in the Keychain.");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut a.new_key).password(true).hint_text("sk-ant-…").desired_width(300.0));
                    if ui.add_enabled(!a.new_key.trim().is_empty(), egui::Button::new("Save key")).clicked() {
                        match crate::ai_shader::store_key(a.new_key.trim()) {
                            Ok(()) => {
                                a.new_key.clear();
                                a.key_error = None;
                                a.key = None;
                            }
                            Err(e) => a.key_error = Some(e),
                        }
                    }
                });
            }
        }
        if let Some(e) = &a.key_error {
            ui.colored_label(WARN, e.as_str());
        }
    });
    if !open {
        a.open = false;
    }
}
