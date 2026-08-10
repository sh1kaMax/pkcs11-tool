use std::{
    path::PathBuf,
    time::Duration,
};

use chrono::Local;
use eframe::egui::{
    self, Align, Button, CollapsingHeader, Color32, Context, CornerRadius, Frame, Layout, Margin, RichText,
    ScrollArea, Stroke, TextEdit, Ui, Vec2,
};
use rfd::FileDialog;

use crate::{
    pkcs11::{default_module_path, Pkcs11Service, TokenObjectInfo, TokenSession, TokenSummary},
    theme,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum CommandTab {
    Format,
    ChangePin,
    Write,
    Read,
}

impl CommandTab {
    const ALL: [Self; 4] = [Self::Format, Self::ChangePin, Self::Write, Self::Read];

    fn title(self) -> &'static str {
        match self {
            Self::Format => "Форматирование",
            Self::ChangePin => "Смена PIN",
            Self::Write => "Запись на токен",
            Self::Read => "Чтение с токена",
        }
    }

}

struct LoginForm {
    module_path: String,
    pin: String,
    selected_index: usize,
}

struct ChangePinForm {
    old_pin: String,
    new_pin: String,
    repeat_pin: String,
}

struct WriteForm {
    label: String,
    file_path: String,
}

struct ReadForm {
    objects: Vec<TokenObjectInfo>,
    selected_index: Option<usize>,
    target_path: String,
}

pub struct TokenStudioApp {
    service: Option<Pkcs11Service>,
    session: Option<TokenSession>,
    tokens: Vec<TokenSummary>,
    login_form: LoginForm,
    active_tab: CommandTab,
    change_pin_form: ChangePinForm,
    write_form: WriteForm,
    read_form: ReadForm,
    loading: bool,
    last_refresh_label: String,
    login_error: Option<String>,
    format_error: Option<String>,
    change_pin_error: Option<String>,
    write_error: Option<String>,
    read_error: Option<String>,
}

impl TokenStudioApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        let mut app = Self {
            service: None,
            session: None,
            tokens: Vec::new(),
            login_form: LoginForm {
                module_path: default_module_path(),
                pin: String::new(),
                selected_index: 0,
            },
            active_tab: CommandTab::Format,
            change_pin_form: ChangePinForm {
                old_pin: String::new(),
                new_pin: String::new(),
                repeat_pin: String::new(),
            },
            write_form: WriteForm {
                label: String::new(),
                file_path: String::new(),
            },
            read_form: ReadForm {
                objects: Vec::new(),
                selected_index: None,
                target_path: String::new(),
            },
            loading: false,
            last_refresh_label: String::from("Токены еще не запрашивались"),
            login_error: None,
            format_error: None,
            change_pin_error: None,
            write_error: None,
            read_error: None,
        };
        app.refresh_tokens();
        app
    }

    fn refresh_tokens(&mut self) {
        self.close_service();
        self.loading = true;
        match Pkcs11Service::new(self.login_form.module_path.clone())
            .and_then(|service| service.list_tokens().map(|tokens| (service, tokens)))
        {
            Ok((service, tokens)) => {
                self.service = Some(service);
                self.tokens = tokens;
                self.login_form.selected_index = 0;
                self.last_refresh_label = format!("Обновлено {}", Local::now().format("%H:%M:%S"));
                self.login_error = None;
            }
            Err(error) => {
                self.service = None;
                self.tokens.clear();
                self.login_error = Some(format!("Не удалось прочитать токены: {error}"));
            }
        }
        self.loading = false;
    }

    fn login(&mut self) {
        let Some(service) = self.service.as_ref() else {
            self.login_error = Some(String::from("Нет PKCS#11-сервиса"));
            return;
        };
        let Some(token) = self.tokens.get(self.login_form.selected_index).cloned() else {
            self.login_error = Some(String::from("Токен не выбран"));
            return;
        };
        if self.login_form.pin.is_empty() {
            self.login_error = Some(String::from("Введите PIN"));
            return;
        }

        match service.login(token.clone(), &self.login_form.pin) {
            Ok(session) => {
                self.session = Some(session);
                self.change_pin_form.old_pin = self.login_form.pin.clone();
                self.read_objects();
                self.login_error = None;
            }
            Err(error) => self.login_error = Some(format!("Не удалось выполнить вход: {error}")),
        }
    }

    fn logout(&mut self) {
        if let Some(session) = self.session.take() {
            session.logout();
        }
        self.login_form.pin.clear();
        self.change_pin_form = ChangePinForm {
            old_pin: String::new(),
            new_pin: String::new(),
            repeat_pin: String::new(),
        };
        self.write_form = WriteForm {
            label: String::new(),
            file_path: String::new(),
        };
        self.read_form = ReadForm {
            objects: Vec::new(),
            selected_index: None,
            target_path: String::new(),
        };
        self.close_service();
        self.refresh_tokens();
        self.clear_action_errors();
    }

    fn close_service(&mut self) {
        if let Some(service) = self.service.take() {
            service.shutdown();
        }
    }

    fn format_token(&mut self) {
        let result = {
            let Some(session) = self.session.as_ref() else {
                self.format_error = Some(String::from("Сначала войдите в токен"));
                return;
            };
            session.format()
        };

        match result {
            Ok(removed) => {
                self.read_objects();
                self.format_error = Some(format!("Готово. Удалено объектов: {removed}"));
            }
            Err(error) => self.format_error = Some(format!("Не удалось очистить токен: {error}")),
        }
    }

    fn change_pin(&mut self) {
        if self.change_pin_form.old_pin.is_empty()
            || self.change_pin_form.new_pin.is_empty()
            || self.change_pin_form.repeat_pin.is_empty()
        {
            self.change_pin_error = Some(String::from("Заполните все поля"));
            return;
        }
        if self.change_pin_form.new_pin != self.change_pin_form.repeat_pin {
            self.change_pin_error = Some(String::from("PIN не совпадает"));
            return;
        }

        let old_pin = self.change_pin_form.old_pin.clone();
        let new_pin = self.change_pin_form.new_pin.clone();
        let result = {
            let Some(session) = self.session.as_ref() else {
                self.change_pin_error = Some(String::from("Сначала войдите в токен"));
                return;
            };
            session.change_pin(&old_pin, &new_pin)
        };

        match result {
            Ok(()) => {
                self.login_form.pin = new_pin.clone();
                self.change_pin_form.old_pin = new_pin;
                self.change_pin_form.new_pin.clear();
                self.change_pin_form.repeat_pin.clear();
                self.change_pin_error = Some(String::from("PIN изменен"));
            }
            Err(error) => self.change_pin_error = Some(format!("Не удалось изменить PIN: {error}")),
        }
    }

    fn write_to_token(&mut self) {
        if self.write_form.label.trim().is_empty() || self.write_form.file_path.trim().is_empty() {
            self.write_error = Some(String::from("Укажите название и файл"));
            return;
        }

        let label = self.write_form.label.trim().to_owned();
        let path = PathBuf::from(self.write_form.file_path.trim());
        let result = {
            let Some(session) = self.session.as_ref() else {
                self.write_error = Some(String::from("Сначала войдите в токен"));
                return;
            };
            session.write_file(&label, &path)
        };

        match result {
            Ok(()) => {
                self.read_objects();
                self.write_error = Some(String::from("Файл записан"));
            }
            Err(error) => self.write_error = Some(format!("Не удалось записать данные: {error}")),
        }
    }

    fn read_objects(&mut self) {
        let result = {
            let Some(session) = self.session.as_ref() else {
                return;
            };
            session.list_objects()
        };

        match result {
            Ok(objects) => {
                self.read_form.objects = objects;
                self.read_form.selected_index = None;
                self.read_error = None;
            }
            Err(error) => self.read_error = Some(format!("Не удалось прочитать список объектов: {error}")),
        }
    }

    fn export_selected_object(&mut self) {
        let Some(index) = self.read_form.selected_index else {
            self.read_error = Some(String::from("Выберите объект"));
            return;
        };
        if self.read_form.target_path.trim().is_empty() {
            self.read_error = Some(String::from("Укажите файл назначения"));
            return;
        }
        let Some(object) = self.read_form.objects.get(index).cloned() else {
            self.read_error = Some(String::from("Объект не найден"));
            return;
        };
        let output_path = PathBuf::from(self.read_form.target_path.trim());
        let result = {
            let Some(session) = self.session.as_ref() else {
                self.read_error = Some(String::from("Сначала войдите в токен"));
                return;
            };
            session.export_object(object.handle, &output_path)
        };

        match result {
            Ok(()) => self.read_error = Some(String::from("Данные выгружены")),
            Err(error) => self.read_error = Some(format!("Не удалось выгрузить объект: {error}")),
        }
    }

    fn clear_action_errors(&mut self) {
        self.login_error = None;
        self.format_error = None;
        self.change_pin_error = None;
        self.write_error = None;
        self.read_error = None;
    }

    fn ui_login(&mut self, _ctx: &Context, ui: &mut Ui) {
        center_card(ui, 660.0, 460.0, |ui| {
            show_card(ui, |ui| {
                header_row(ui, "Вход", true);
                ui.add_space(8.0);

                CollapsingHeader::new(format!("Токены ({})", self.tokens.len()))
                    .default_open(true)
                    .show(ui, |ui| {
                        if self.tokens.is_empty() {
                            ui.label(RichText::new("Нет токенов").color(theme::WARNING));
                        } else {
                            ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                                for (index, token) in self.tokens.iter().enumerate() {
                                    let label = format!("{}  {}", token.label, token.serial);
                                    if ui
                                        .add(list_button(self.login_form.selected_index == index, &label))
                                        .clicked()
                                    {
                                        self.login_form.selected_index = index;
                                    }
                                }
                            });
                        }
                    });

                ui.add_space(14.0);
                ui.add(TextEdit::singleline(&mut self.login_form.pin).password(true).hint_text("PIN"));
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if ui.add(accent_button("Войти")).clicked() {
                        self.login();
                    }
                    if ui.add(outline_button("Обновить")).clicked() && !self.loading {
                        self.refresh_tokens();
                    }
                });
                ui.add_space(8.0);
                ui.label(RichText::new(&self.last_refresh_label).color(theme::TEXT_MUTED));
                if let Some(message) = &self.login_error {
                    ui.add_space(8.0);
                    ui.label(RichText::new(message).color(theme::TEXT_MUTED));
                }
            });
        });
    }

    fn ui_dashboard(&mut self, ctx: &Context, ui: &mut Ui) {
        let token = self.session.as_ref().map(|session| session.token.clone());
        if let Some(token) = token {
            center_card(ui, 880.0, 720.0, |ui| {
                show_card(ui, |ui| {
                    header_row(ui, &token.label, false);
                    ui.add_space(8.0);
                    ScrollArea::vertical().max_height(630.0).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.add(small_button("Домой")).clicked() {
                                self.logout();
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.add(outline_button("Обновить")).clicked() {
                                    self.read_objects();
                                }
                            });
                        });
                        ui.add_space(10.0);
                        CollapsingHeader::new("Команды")
                            .default_open(true)
                            .show(ui, |ui| {
                                for tab in CommandTab::ALL {
                                    if ui.add(list_button(self.active_tab == tab, tab.title())).clicked() {
                                        self.active_tab = tab;
                                        if self.active_tab == CommandTab::Read {
                                            self.read_objects();
                                        }
                                    }
                                }
                            });
                        ui.add_space(12.0);
                        match self.active_tab {
                            CommandTab::Format => self.ui_format(ui),
                            CommandTab::ChangePin => self.ui_change_pin(ui),
                            CommandTab::Write => self.ui_write(ui),
                            CommandTab::Read => self.ui_read(ui),
                        }
                    });
                });
            });
        }

        ctx.request_repaint_after(Duration::from_millis(16));
    }

    fn ui_format(&mut self, ui: &mut Ui) {
        if ui.add(accent_button("Выполнить")).clicked() {
            self.format_token();
        }
        if let Some(message) = &self.format_error {
            ui.add_space(8.0);
            ui.label(RichText::new(message).color(theme::TEXT_MUTED));
        }
    }

    fn ui_change_pin(&mut self, ui: &mut Ui) {
        ui.add(TextEdit::singleline(&mut self.change_pin_form.old_pin).password(true).hint_text("Старый PIN"));
        ui.add(TextEdit::singleline(&mut self.change_pin_form.new_pin).password(true).hint_text("Новый PIN"));
        ui.add(TextEdit::singleline(&mut self.change_pin_form.repeat_pin).password(true).hint_text("Повтор PIN"));
        ui.add_space(10.0);
        if ui.add(accent_button("Выполнить")).clicked() {
            self.change_pin();
        }
        if let Some(message) = &self.change_pin_error {
            ui.add_space(8.0);
            ui.label(RichText::new(message).color(theme::TEXT_MUTED));
        }
    }

    fn ui_write(&mut self, ui: &mut Ui) {
        ui.add(TextEdit::singleline(&mut self.write_form.label).hint_text("Название"));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_sized(
                [ui.available_width() - 120.0, 40.0],
                TextEdit::singleline(&mut self.write_form.file_path).hint_text("Файл"),
            );
            if ui.add(outline_button("Файл")).clicked() {
                if let Some(path) = FileDialog::new().pick_file() {
                    self.write_form.file_path = path.display().to_string();
                }
            }
        });
        ui.add_space(10.0);
        if ui.add(accent_button("Выполнить")).clicked() {
            self.write_to_token();
        }
        if let Some(message) = &self.write_error {
            ui.add_space(8.0);
            ui.label(RichText::new(message).color(theme::TEXT_MUTED));
        }
    }

    fn ui_read(&mut self, ui: &mut Ui) {
        CollapsingHeader::new(format!("Объекты ({})", self.read_form.objects.len()))
            .default_open(true)
            .show(ui, |ui| {
                ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    for (index, object) in self.read_form.objects.iter().enumerate() {
                        let title = format!("{}  {} байт", object.label, object.size);
                        if ui
                            .add(list_button(self.read_form.selected_index == Some(index), &title))
                            .clicked()
                        {
                            self.read_form.selected_index = Some(index);
                        }
                    }
                });
            });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_sized(
                [ui.available_width() - 120.0, 40.0],
                TextEdit::singleline(&mut self.read_form.target_path).hint_text("Куда сохранить"),
            );
            if ui.add(outline_button("Файл")).clicked() {
                if let Some(path) = FileDialog::new().save_file() {
                    self.read_form.target_path = path.display().to_string();
                }
            }
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.add(accent_button("Выполнить")).clicked() {
                self.export_selected_object();
            }
            if ui.add(outline_button("Обновить")).clicked() {
                self.read_objects();
            }
        });
        if let Some(message) = &self.read_error {
            ui.add_space(8.0);
            ui.label(RichText::new(message).color(theme::TEXT_MUTED));
        }
    }

    fn draw_background(&self, ui: &mut Ui) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
    }

}

impl eframe::App for TokenStudioApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.draw_background(ui);

        let ctx = ui.ctx().clone();
        ui.scope(|ui| {
            ui.set_width(ui.available_width());
            if self.session.is_some() {
                self.ui_dashboard(&ctx, ui);
            } else {
                self.ui_login(&ctx, ui);
            }
        });
    }
}

fn show_card<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(Color32::from_rgba_premultiplied(
            theme::PANEL.r(),
            theme::PANEL.g(),
            theme::PANEL.b(),
            242,
        ))
        .stroke(Stroke::new(1.0, Color32::from_rgba_premultiplied(20, 20, 20, 18)))
        .corner_radius(CornerRadius::same(28))
        .inner_margin(Margin::same(20))
        .show(ui, add_contents)
        .inner
}

fn center_card<R>(ui: &mut Ui, width: f32, height: f32, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    let top = ((ui.available_height() - height) * 0.5).max(0.0);
    ui.add_space(top);
    ui.horizontal(|ui| {
        let side = ((ui.available_width() - width) * 0.5).max(0.0);
        ui.add_space(side);
        ui.vertical(|ui| {
            ui.set_width(width);
            add_contents(ui)
        })
        .inner
    })
    .inner
}

fn list_button(text_selected: bool, text: &str) -> Button<'_> {
    Button::new(
        RichText::new(text)
            .color(if text_selected { theme::PANEL } else { theme::TEXT })
            .strong(),
    )
    .fill(if text_selected { theme::TURQUOISE } else { theme::PANEL })
    .stroke(Stroke::new(1.0, Color32::from_rgba_premultiplied(20, 20, 20, 22)))
    .corner_radius(CornerRadius::same(12))
    .min_size(Vec2::new(0.0, 34.0))
}

fn accent_button(text: &str) -> Button<'_> {
    Button::new(RichText::new(text).strong().color(theme::PANEL))
        .fill(theme::TURQUOISE)
        .stroke(Stroke::new(1.0, Color32::from_rgba_premultiplied(20, 20, 20, 50)))
        .corner_radius(CornerRadius::same(14))
        .min_size(Vec2::new(0.0, 38.0))
}

fn outline_button(text: &str) -> Button<'_> {
    Button::new(RichText::new(text).color(theme::TEXT))
        .fill(theme::PANEL)
        .stroke(Stroke::new(1.0, Color32::from_rgba_premultiplied(20, 20, 20, 48)))
        .corner_radius(CornerRadius::same(14))
        .min_size(Vec2::new(0.0, 38.0))
}

fn small_button(text: &str) -> Button<'_> {
    Button::new(RichText::new(text).color(theme::TEXT))
        .fill(theme::PANEL)
        .stroke(Stroke::new(1.0, Color32::from_rgba_premultiplied(20, 20, 20, 48)))
        .corner_radius(CornerRadius::same(12))
        .min_size(Vec2::new(64.0, 30.0))
}

fn header_row(ui: &mut Ui, title: &str, allow_close: bool) {
    let response = ui
        .horizontal(|ui| {
            ui.label(RichText::new(title).text_style(egui::TextStyle::Heading));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if allow_close && ui.add(small_button("Закрыть")).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        })
        .response;

    if response.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}

impl Drop for TokenStudioApp {
    fn drop(&mut self) {
        if let Some(session) = self.session.take() {
            session.logout();
        }
        self.close_service();
    }
}
