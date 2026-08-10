use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use chrono::Local;
use eframe::egui::{
    self, Align, Align2, Area, Button, CollapsingHeader, Color32, Context, CornerRadius, Frame, Id, Layout, Margin,
    RichText, ScrollArea, Stroke, TextEdit, Ui, Vec2,
};
use rfd::FileDialog;

use crate::{
    pkcs11::{default_module_path, Pkcs11Service, ServiceError, TokenObjectInfo, TokenSession, TokenSummary},
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

#[derive(Clone, Copy)]
enum ToastKind {
    Success,
    Error,
    Info,
}

struct Toast {
    id: u64,
    title: String,
    body: String,
    kind: ToastKind,
    created_at: Instant,
    ttl: Duration,
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
    toasts: Vec<Toast>,
    next_toast_id: u64,
    loading: bool,
    last_refresh_label: String,
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
            toasts: Vec::new(),
            next_toast_id: 1,
            loading: false,
            last_refresh_label: String::from("Токены еще не запрашивались"),
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
                self.push_toast(ToastKind::Success, "Список токенов обновлен", "Подключенные устройства успешно прочитаны.");
            }
            Err(error) => {
                self.service = None;
                self.tokens.clear();
                self.push_error("Не удалось прочитать токены", error);
            }
        }
        self.loading = false;
    }

    fn login(&mut self) {
        let Some(service) = self.service.as_ref() else {
            self.push_toast(ToastKind::Error, "Нет PKCS#11-сервиса", "Сначала укажите корректный путь к PKCS#11-модулю.");
            return;
        };
        let Some(token) = self.tokens.get(self.login_form.selected_index).cloned() else {
            self.push_toast(ToastKind::Error, "Токен не выбран", "Подключите Рутокен и обновите список.");
            return;
        };
        if self.login_form.pin.is_empty() {
            self.push_toast(ToastKind::Error, "Пустой PIN", "Введите PIN пользователя для входа в токен.");
            return;
        }

        match service.login(token.clone(), &self.login_form.pin) {
            Ok(session) => {
                self.session = Some(session);
                self.change_pin_form.old_pin = self.login_form.pin.clone();
                self.read_objects();
                self.push_toast(
                    ToastKind::Success,
                    "Вход выполнен",
                    &format!("Активная сессия открыта для токена {}", token.label),
                );
            }
            Err(error) => self.push_error("Не удалось выполнить вход", error),
        }
    }

    fn logout(&mut self) {
        if let Some(session) = self.session.take() {
            session.logout();
            self.push_toast(ToastKind::Info, "Сессия завершена", "Пользователь вышел из токена, ресурсы освобождены.");
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
    }

    fn close_service(&mut self) {
        if let Some(service) = self.service.take() {
            service.shutdown();
        }
    }

    fn format_token(&mut self) {
        let result = {
            let Some(session) = self.session.as_ref() else {
                self.push_toast(ToastKind::Error, "Нет активной сессии", "Сначала войдите в токен.");
                return;
            };
            session.format()
        };

        match result {
            Ok(removed) => {
                self.read_objects();
                self.push_toast(
                    ToastKind::Success,
                    "Форматирование завершено",
                    &format!("Удалено объектов: {removed}"),
                );
            }
            Err(error) => self.push_error("Не удалось очистить токен", error),
        }
    }

    fn change_pin(&mut self) {
        if self.change_pin_form.old_pin.is_empty()
            || self.change_pin_form.new_pin.is_empty()
            || self.change_pin_form.repeat_pin.is_empty()
        {
            self.push_toast(ToastKind::Error, "Не все поля заполнены", "Для смены PIN нужно заполнить старый и новый PIN.");
            return;
        }
        if self.change_pin_form.new_pin != self.change_pin_form.repeat_pin {
            self.push_toast(ToastKind::Error, "PIN не совпадает", "Повтор нового PIN отличается от введенного значения.");
            return;
        }

        let old_pin = self.change_pin_form.old_pin.clone();
        let new_pin = self.change_pin_form.new_pin.clone();
        let result = {
            let Some(session) = self.session.as_ref() else {
                self.push_toast(ToastKind::Error, "Нет активной сессии", "Сначала войдите в токен.");
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
                self.push_toast(ToastKind::Success, "PIN изменен", "Новый PIN записан в токен.");
            }
            Err(error) => self.push_error("Не удалось изменить PIN", error),
        }
    }

    fn write_to_token(&mut self) {
        if self.write_form.label.trim().is_empty() || self.write_form.file_path.trim().is_empty() {
            self.push_toast(ToastKind::Error, "Не хватает данных", "Укажите название объекта и выберите файл для записи.");
            return;
        }

        let label = self.write_form.label.trim().to_owned();
        let path = PathBuf::from(self.write_form.file_path.trim());
        let result = {
            let Some(session) = self.session.as_ref() else {
                self.push_toast(ToastKind::Error, "Нет активной сессии", "Сначала войдите в токен.");
                return;
            };
            session.write_file(&label, &path)
        };

        match result {
            Ok(()) => {
                self.read_objects();
                self.push_toast(ToastKind::Success, "Файл записан", "Новый объект успешно создан на токене.");
            }
            Err(error) => self.push_error("Не удалось записать данные", error),
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
            }
            Err(error) => self.push_error("Не удалось прочитать список объектов", error),
        }
    }

    fn export_selected_object(&mut self) {
        let Some(index) = self.read_form.selected_index else {
            self.push_toast(ToastKind::Error, "Объект не выбран", "Выберите объект в списке для чтения.");
            return;
        };
        if self.read_form.target_path.trim().is_empty() {
            self.push_toast(ToastKind::Error, "Путь не выбран", "Укажите файл, в который нужно выгрузить данные.");
            return;
        }
        let Some(object) = self.read_form.objects.get(index).cloned() else {
            self.push_toast(ToastKind::Error, "Объект не найден", "Список объектов устарел, перечитайте токен.");
            return;
        };
        let output_path = PathBuf::from(self.read_form.target_path.trim());
        let result = {
            let Some(session) = self.session.as_ref() else {
                self.push_toast(ToastKind::Error, "Нет активной сессии", "Сначала войдите в токен.");
                return;
            };
            session.export_object(object.handle, &output_path)
        };

        match result {
            Ok(()) => self.push_toast(ToastKind::Success, "Данные выгружены", "Выбранный объект записан в указанный файл."),
            Err(error) => self.push_error("Не удалось выгрузить объект", error),
        }
    }

    fn push_error(&mut self, title: &str, error: ServiceError) {
        self.push_toast(ToastKind::Error, title, &error.to_string());
    }

    fn push_toast(&mut self, kind: ToastKind, title: &str, body: &str) {
        self.toasts.push(Toast {
            id: self.next_toast_id,
            title: title.to_owned(),
            body: body.to_owned(),
            kind,
            created_at: Instant::now(),
            ttl: Duration::from_secs(4),
        });
        self.next_toast_id += 1;
    }

    fn ui_login(&mut self, _ctx: &Context, ui: &mut Ui) {
        center_card(ui, 560.0, |ui| {
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
            });
        });
    }

    fn ui_dashboard(&mut self, ctx: &Context, ui: &mut Ui) {
        let token = self.session.as_ref().map(|session| session.token.clone());
        if let Some(token) = token {
            center_card(ui, 720.0, |ui| {
                show_card(ui, |ui| {
                    header_row(ui, &token.label, false);
                    ui.add_space(8.0);
                    ScrollArea::vertical().max_height(560.0).show(ui, |ui| {
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
    }

    fn ui_change_pin(&mut self, ui: &mut Ui) {
        ui.add(TextEdit::singleline(&mut self.change_pin_form.old_pin).password(true).hint_text("Старый PIN"));
        ui.add(TextEdit::singleline(&mut self.change_pin_form.new_pin).password(true).hint_text("Новый PIN"));
        ui.add(TextEdit::singleline(&mut self.change_pin_form.repeat_pin).password(true).hint_text("Повтор PIN"));
        ui.add_space(10.0);
        if ui.add(accent_button("Выполнить")).clicked() {
            self.change_pin();
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
    }

    fn draw_background(&self, ui: &mut Ui) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, Color32::TRANSPARENT);
    }

    fn draw_toasts(&mut self, ctx: &Context) {
        let now = Instant::now();
        self.toasts.retain(|toast| now.duration_since(toast.created_at) < toast.ttl);

        for (index, toast) in self.toasts.iter().enumerate() {
            let age = now.duration_since(toast.created_at);
            let alpha = if age > toast.ttl.saturating_sub(Duration::from_millis(700)) {
                1.0 - ((age - (toast.ttl - Duration::from_millis(700))).as_secs_f32() / 0.7)
            } else {
                1.0
            }
            .clamp(0.0, 1.0);

            let color = match toast.kind {
                ToastKind::Success => theme::TEXT,
                ToastKind::Error => theme::TEXT,
                ToastKind::Info => theme::TEXT_MUTED,
            };

            Area::new(Id::new(("toast", toast.id)))
                .anchor(Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -16.0 - index as f32 * 62.0))
                .show(ctx, |ui| {
                    Frame::new()
                        .fill(Color32::from_rgba_premultiplied(252, 252, 252, (235.0 * alpha) as u8))
                        .stroke(Stroke::new(1.0, Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), (40.0 * alpha) as u8)))
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(Margin::same(10))
                        .show(ui, |ui| {
                            ui.set_width(220.0);
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&toast.title).strong().size(13.0));
                                    ui.label(RichText::new(&toast.body).color(theme::TEXT_MUTED).size(11.0));
                                });
                            });
                        });
                });
        }
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

        self.draw_toasts(&ctx);
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

fn center_card<R>(ui: &mut Ui, width: f32, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
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
            .color(if text_selected { theme::BG_DARKEST } else { theme::TEXT })
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
