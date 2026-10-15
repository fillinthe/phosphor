//! Account panel: sign in, and (Pro) projects stored in your phosphorfx.com account.

use super::*;

/// Text fields and pending confirmations for the Account panel.
#[derive(Default)]
pub struct AccountUi {
    pub email: String,
    pub password: String,
    pub code: String,
    /// Name the current project is saved under.
    pub name: String,
    /// Project waiting for a second click on Delete.
    pub confirm_delete: Option<String>,
}

pub(crate) fn account(ui: &mut egui::Ui, st: &mut State) {
    let ctx = ui.ctx().clone();
    let State { account: a, account_ui: f, comp, cloud_project, project_path, .. } = st;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Your account").strong());
        if a.busy > 0 {
            ui.spinner();
        }
    });
    if !a.error.is_empty() {
        ui.colored_label(WARN, &a.error);
    }
    if !a.checked {
        hint(ui, "Checking your sign-in…");
        return;
    }
    let Some(user) = a.user.clone() else {
        sign_in_form(ui, &ctx, a, f);
        return;
    };
    ui.horizontal(|ui| {
        ui.label(&user.email);
        let plan = if a.can_store() { RichText::new("Pro").color(ACCENT).strong() } else { RichText::new("Free").color(Color32::GRAY) };
        ui.label(plan);
        if ui.small_button("Account page").clicked() {
            a.open_web(&ctx, "/account/");
        }
        if !Account::web() && ui.small_button("Sign out").clicked() {
            a.sign_out(&ctx);
        }
    });
    ui.separator();
    if !a.can_store() {
        ui.label("Saving projects to your account is part of Phosphor Pro: your shows open on any computer you sign in on.");
        if Account::web() {
            hint(ui, "In the browser, projects are kept only in your account.");
        } else {
            hint(ui, "File › Save keeps projects on this computer, free.");
        }
        if ui.button(label(icon::STAR, "See Pro")).clicked() {
            a.open_web(&ctx, "/#pricing");
        }
        return;
    }
    if let Some((used, max)) = a.storage {
        let frac = if max > 0 { used as f32 / max as f32 } else { 0.0 };
        ui.add(egui::ProgressBar::new(frac).desired_height(6.0).fill(ACCENT.gamma_multiply(0.6)));
        hint(ui, format!("{:.1} MB of {} MB used", used as f64 / 1_048_576.0, max >> 20));
    }
    if f.name.is_empty() {
        f.name = cloud_project
            .as_ref()
            .map(|(_, n)| n.clone())
            .or_else(|| project_path.as_deref().and_then(|p| std::path::Path::new(p).file_stem()).map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "My show".into());
    }
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut f.name).hint_text("Project name").desired_width(180.0));
        let name = f.name.trim().to_string();
        match cloud_project.clone() {
            Some((id, _)) => {
                if ui.button(label(icon::CLOUD_ARROW_UP, "Save")).on_hover_text("Replace the copy in your account").clicked() && !name.is_empty() {
                    a.save(&ctx, &id, &name, &comp.to_json());
                }
                if ui.button("Save as new").clicked() && !name.is_empty() {
                    a.save(&ctx, "", &name, &comp.to_json());
                }
            }
            None => {
                if ui.button(label(icon::CLOUD_ARROW_UP, "Save to account")).clicked() && !name.is_empty() {
                    a.save(&ctx, "", &name, &comp.to_json());
                }
            }
        }
    });
    ui.add_space(4.0);
    if a.projects.is_empty() {
        hint(ui, "Nothing saved in your account yet.");
    }
    let mut open = None;
    let mut delete = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for p in &a.projects {
            let current = cloud_project.as_ref().is_some_and(|(id, _)| *id == p.id);
            ui.horizontal(|ui| {
                if ui.button(label(icon::FOLDER_OPEN, "Open")).clicked() {
                    open = Some(p.id.clone());
                }
                let name = if current { RichText::new(&p.name).color(ACCENT) } else { RichText::new(&p.name) };
                ui.label(name);
                hint(ui, format!("{} · {:.0} KB", ago_secs(p.updated), p.size as f64 / 1024.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let armed = f.confirm_delete.as_deref() == Some(p.id.as_str());
                    let text = if armed { RichText::new("Delete for good?").color(WARN) } else { RichText::new(icon::TRASH) };
                    if ui.small_button(text).on_hover_text("Delete from your account (click twice)").clicked() {
                        if armed {
                            delete = Some(p.id.clone());
                        } else {
                            f.confirm_delete = Some(p.id.clone());
                        }
                    }
                });
            });
        }
    });
    if let Some(id) = open {
        a.open(&ctx, &id);
    }
    if let Some(id) = delete {
        f.confirm_delete = None;
        if cloud_project.as_ref().is_some_and(|(c, _)| *c == id) {
            *cloud_project = None;
        }
        a.delete(&ctx, &id);
    }
}

fn sign_in_form(ui: &mut egui::Ui, ctx: &egui::Context, a: &mut Account, f: &mut AccountUi) {
    if Account::web() {
        ui.label("Sign in on phosphorfx.com to use your account.");
        if ui.button("Sign in").clicked() {
            a.open_web(ctx, "/signin/?next=/app/");
        }
        return;
    }
    ui.label("Sign in with your phosphorfx.com account to keep projects there (Pro).");
    egui::Grid::new("signin").num_columns(2).show(ui, |ui| {
        ui.label("Email");
        ui.add(egui::TextEdit::singleline(&mut f.email).desired_width(220.0));
        ui.end_row();
        ui.label("Password");
        let pw = ui.add(egui::TextEdit::singleline(&mut f.password).password(true).desired_width(220.0));
        ui.end_row();
        if a.need_code {
            ui.label("Code");
            ui.add(egui::TextEdit::singleline(&mut f.code).hint_text("from your authenticator").desired_width(220.0));
            ui.end_row();
        }
        let enter = pw.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.label("");
        if (ui.button("Sign in").clicked() || enter) && !f.email.trim().is_empty() && !f.password.is_empty() {
            a.sign_in(ctx, &f.email, &f.password, &f.code);
            f.password.clear();
        }
        ui.end_row();
    });
    ui.horizontal(|ui| {
        if ui.link("Create a free account").clicked() {
            a.open_web(ctx, "/signin/?signup");
        }
        if ui.link("Forgot your password?").clicked() {
            a.open_web(ctx, "/signin/?forgot");
        }
    });
}
