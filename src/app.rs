use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use chrono::Local;
use eframe::egui::{
    self, Align, Align2, Area, Button, CentralPanel, Color32, ComboBox, Context, CornerRadius, Frame, Id, Layout,
    Margin, RichText, ScrollArea, Sense, Stroke, TextEdit, Ui, Vec2,
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

    fn description(self) -> &'static str {
        match self {
            Self::Format => "Быстрая очистка пользовательских объектов на токене.",
            Self::ChangePin => "Обновление пользовательского PIN-кода без выхода из рабочей сессии.",
            Self::Write => "Запись файла на токен как защищенного PKCS#11 DATA-объекта.",
            Self::Read => "Просмотр содержимого токена и выгрузка выбранного объекта в файл.",
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

        match service.login(token.clone(), self.login_form.pin.trim()) {
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
            ttl: Duration::from_secs(6),
        });
        self.next_toast_id += 1;
    }

    fn ui_login(&mut self, ctx: &Context, ui: &mut Ui) {
        ui.add_space(24.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("PKCS11 Token Studio").text_style(egui::TextStyle::Name("Hero".into())));
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Компактный центр управления Рутокеном с темным интерфейсом, анимацией и быстрыми PKCS#11-командами.")
                        .color(theme::TEXT_MUTED),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let pulse = (ctx.input(|i| i.time) as f32).sin() * 0.5 + 0.5;
                let color = Color32::from_rgba_premultiplied(34, 219, 196, (90.0 + pulse * 90.0) as u8);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(68.0, 68.0), Sense::hover());
                ui.painter().circle_filled(rect.center(), 30.0 + pulse * 6.0, color);
                ui.painter().circle_stroke(rect.center(), 30.0 + pulse * 6.0, Stroke::new(1.5, theme::TURQUOISE_SOFT));
            });
        });

        ui.add_space(20.0);
        self.card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("PKCS#11 модуль").text_style(egui::TextStyle::Name("Section".into())));
                    ui.label(RichText::new("Укажите путь до `rtpkcs11ecp` и обновите список устройств.").color(theme::TEXT_MUTED));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(accent_button("Обновить список")).clicked() && !self.loading {
                        self.refresh_tokens();
                    }
                });
            });
            ui.add_space(12.0);
            ui.add(TextEdit::singleline(&mut self.login_form.module_path).hint_text("Путь до PKCS#11-библиотеки"));
            ui.label(RichText::new(&self.last_refresh_label).color(theme::TEXT_MUTED));
        });

        ui.add_space(18.0);
        ui.columns(2, |columns| {
            self.card(&mut columns[0], |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Подключенные токены").text_style(egui::TextStyle::Name("Section".into())));
                    ui.label(RichText::new(format!("{} шт.", self.tokens.len())).color(theme::TURQUOISE_SOFT));
                });
                ui.add_space(8.0);
                if self.tokens.is_empty() {
                    ui.label(RichText::new("Токены не найдены. Проверьте драйвер Рутокен и путь к PKCS#11-модулю.").color(theme::WARNING));
                } else {
                    ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        for (index, token) in self.tokens.iter().enumerate() {
                            let selected = self.login_form.selected_index == index;
                            let frame = Frame::new()
                                .fill(if selected { theme::PANEL_ALT } else { theme::PANEL })
                                .stroke(Stroke::new(1.0, if selected { theme::TURQUOISE } else { Color32::TRANSPARENT }))
                                .corner_radius(CornerRadius::same(18))
                                .inner_margin(Margin::same(16));
                            frame.show(ui, |ui| {
                                let response = ui
                                    .horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(RichText::new(&token.label).strong().size(17.0));
                                            ui.label(RichText::new(format!("{} • {}", token.model, token.manufacturer)).color(theme::TEXT_MUTED));
                                            ui.label(RichText::new(format!("Серийный номер: {}", token.serial)).color(theme::TEXT_MUTED));
                                        });
                                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                            ui.label(RichText::new(format!("{:?}", token.slot)).color(theme::TURQUOISE_SOFT));
                                        });
                                    })
                                    .response;
                                if response.clicked() {
                                    self.login_form.selected_index = index;
                                }
                            });
                            ui.add_space(10.0);
                        }
                    });
                }
            });

            self.card(&mut columns[1], |ui| {
                ui.label(RichText::new("Вход в токен").text_style(egui::TextStyle::Name("Section".into())));
                ui.label(RichText::new("Сначала выбирается токен, затем выполняется вход пользователя.").color(theme::TEXT_MUTED));
                ui.add_space(16.0);
                let selected_text = self
                    .tokens
                    .get(self.login_form.selected_index)
                    .map(|token| token.label.clone())
                    .unwrap_or_else(|| "Нет доступных токенов".into());
                ComboBox::from_label("Выбранный токен")
                    .selected_text(selected_text)
                    .show_ui(ui, |ui| {
                        for (index, token) in self.tokens.iter().enumerate() {
                            ui.selectable_value(&mut self.login_form.selected_index, index, &token.label);
                        }
                    });
                ui.add_space(8.0);
                ui.add(TextEdit::singleline(&mut self.login_form.pin).password(true).hint_text("PIN пользователя"));
                ui.add_space(16.0);
                if ui.add(accent_button("Войти в токен")).clicked() {
                    self.login();
                }
                ui.add_space(12.0);
                ui.label(RichText::new("После входа появляется командный центр. При выходе и закрытии приложения сессия завершается автоматически.").color(theme::TEXT_MUTED));
            });
        });
    }

    fn ui_dashboard(&mut self, ctx: &Context, ui: &mut Ui) {
        let token = self.session.as_ref().map(|session| session.token.clone());
        if let Some(token) = token {
            self.card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Командный центр").text_style(egui::TextStyle::Name("Hero".into())));
                        ui.label(
                            RichText::new(format!(
                                "{} • {} • {}",
                                token.label, token.model, token.serial
                            ))
                            .color(theme::TEXT_MUTED),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(outline_button("Выйти с токена")).clicked() {
                            self.logout();
                        }
                        if ui.add(accent_button("Обновить объекты")).clicked() {
                            self.read_objects();
                        }
                    });
                });
            });

            ui.add_space(18.0);
            ui.columns(2, |columns| {
                columns[0].set_width(270.0);
                self.card(&mut columns[0], |ui| {
                    ui.label(RichText::new("Операции").text_style(egui::TextStyle::Name("Section".into())));
                    ui.label(RichText::new("Сначала выберите команду, затем заполните только нужные поля.").color(theme::TEXT_MUTED));
                    ui.add_space(10.0);
                    for tab in CommandTab::ALL {
                        let selected = self.active_tab == tab;
                        let mut button = Button::new(
                            RichText::new(tab.title())
                                .strong()
                                .color(if selected { theme::BG_DARKEST } else { theme::TEXT }),
                        )
                        .min_size(Vec2::new(ui.available_width(), 54.0));
                        button = if selected {
                            button.fill(theme::TURQUOISE)
                        } else {
                            button.fill(theme::PANEL)
                        };
                        if ui.add(button).clicked() {
                            self.active_tab = tab;
                            if self.active_tab == CommandTab::Read {
                                self.read_objects();
                            }
                        }
                        ui.add_space(6.0);
                    }
                });

                self.card(&mut columns[1], |ui| {
                    ui.label(RichText::new(self.active_tab.title()).text_style(egui::TextStyle::Name("Section".into())));
                    ui.label(RichText::new(self.active_tab.description()).color(theme::TEXT_MUTED));
                    ui.add_space(18.0);
                    match self.active_tab {
                        CommandTab::Format => self.ui_format(ui),
                        CommandTab::ChangePin => self.ui_change_pin(ui),
                        CommandTab::Write => self.ui_write(ui),
                        CommandTab::Read => self.ui_read(ui),
                    }
                });
            });
        }

        ctx.request_repaint_after(Duration::from_millis(16));
    }

    fn ui_format(&mut self, ui: &mut Ui) {
        ui.label("Команда удаляет пользовательские объекты и очищает рабочее пространство токена.");
        ui.add_space(16.0);
        if ui.add(accent_button("Выполнить форматирование")).clicked() {
            self.format_token();
        }
    }

    fn ui_change_pin(&mut self, ui: &mut Ui) {
        ui.add(TextEdit::singleline(&mut self.change_pin_form.old_pin).password(true).hint_text("Старый PIN"));
        ui.add(TextEdit::singleline(&mut self.change_pin_form.new_pin).password(true).hint_text("Новый PIN"));
        ui.add(TextEdit::singleline(&mut self.change_pin_form.repeat_pin).password(true).hint_text("Повтор нового PIN"));
        ui.add_space(16.0);
        if ui.add(accent_button("Выполнить смену PIN")).clicked() {
            self.change_pin();
        }
    }

    fn ui_write(&mut self, ui: &mut Ui) {
        ui.add(TextEdit::singleline(&mut self.write_form.label).hint_text("Название объекта на токене"));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_sized(
                [ui.available_width() - 170.0, 44.0],
                TextEdit::singleline(&mut self.write_form.file_path).hint_text("Файл для записи"),
            );
            if ui.add(outline_button("Выбрать файл")).clicked() {
                if let Some(path) = FileDialog::new().pick_file() {
                    self.write_form.file_path = path.display().to_string();
                }
            }
        });
        ui.add_space(16.0);
        if ui.add(accent_button("Записать на токен")).clicked() {
            self.write_to_token();
        }
    }

    fn ui_read(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui.add(outline_button("Перечитать токен")).clicked() {
                self.read_objects();
            }
            ui.label(RichText::new(format!("Найдено объектов: {}", self.read_form.objects.len())).color(theme::TEXT_MUTED));
        });
        ui.add_space(12.0);
        ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
            for (index, object) in self.read_form.objects.iter().enumerate() {
                let selected = self.read_form.selected_index == Some(index);
                let fill = if selected { theme::PANEL_ALT } else { theme::PANEL };
                Frame::new()
                    .fill(fill)
                    .stroke(Stroke::new(1.0, if selected { theme::TURQUOISE } else { Color32::TRANSPARENT }))
                    .corner_radius(CornerRadius::same(18))
                    .inner_margin(Margin::same(16))
                    .show(ui, |ui| {
                        let response = ui
                            .horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&object.label).strong());
                                    ui.label(RichText::new(format!("{} • {} байт", object.class_name, object.size)).color(theme::TEXT_MUTED));
                                });
                            })
                            .response;
                        if response.clicked() {
                            self.read_form.selected_index = Some(index);
                        }
                    });
                ui.add_space(8.0);
            }
        });
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.add_sized(
                [ui.available_width() - 180.0, 44.0],
                TextEdit::singleline(&mut self.read_form.target_path).hint_text("Файл для выгрузки"),
            );
            if ui.add(outline_button("Куда сохранить")).clicked() {
                if let Some(path) = FileDialog::new().save_file() {
                    self.read_form.target_path = path.display().to_string();
                }
            }
        });
        ui.add_space(16.0);
        if ui.add(accent_button("Выполнить чтение")).clicked() {
            self.export_selected_object();
        }
    }

    fn draw_background(&self, ctx: &Context) {
        CentralPanel::default()
            .frame(Frame::new().fill(theme::BG_DARKEST))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let painter = ui.painter();
                painter.rect_filled(rect, 0.0, theme::BG_DARKEST);

                let top = rect.left_top() + egui::vec2(180.0, 120.0);
                let bottom = rect.right_bottom() - egui::vec2(220.0, 140.0);
                painter.circle_filled(top, 220.0, Color32::from_rgba_premultiplied(34, 219, 196, 18));
                painter.circle_filled(bottom, 280.0, Color32::from_rgba_premultiplied(13, 91, 112, 26));
            });
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
                ToastKind::Success => theme::TURQUOISE,
                ToastKind::Error => theme::DANGER,
                ToastKind::Info => theme::WARNING,
            };

            Area::new(Id::new(("toast", toast.id)))
                .anchor(Align2::RIGHT_TOP, egui::vec2(-22.0, 22.0 + index as f32 * 92.0))
                .show(ctx, |ui| {
                    Frame::new()
                        .fill(Color32::from_rgba_premultiplied(15, 34, 41, (230.0 * alpha) as u8))
                        .stroke(Stroke::new(1.0, Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), (255.0 * alpha) as u8)))
                        .corner_radius(CornerRadius::same(18))
                        .inner_margin(Margin::same(14))
                        .show(ui, |ui| {
                            ui.set_width(320.0);
                            ui.horizontal(|ui| {
                                let (rect, _) = ui.allocate_exact_size(Vec2::new(12.0, 12.0), Sense::hover());
                                ui.painter().circle_filled(rect.center(), 5.0, Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), (255.0 * alpha) as u8));
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&toast.title).strong());
                                    ui.label(RichText::new(&toast.body).color(theme::TEXT_MUTED));
                                });
                            });
                        });
                });
        }
    }

    fn card<R>(&self, ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
        Frame::new()
            .fill(Color32::from_rgba_premultiplied(
                theme::PANEL.r(),
                theme::PANEL.g(),
                theme::PANEL.b(),
                242,
            ))
            .stroke(Stroke::new(1.0, Color32::from_rgba_premultiplied(34, 219, 196, 34)))
            .corner_radius(CornerRadius::same(24))
            .inner_margin(Margin::same(20))
            .show(ui, add_contents)
            .inner
    }
}

impl eframe::App for TokenStudioApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.draw_background(ctx);

        CentralPanel::default()
            .frame(Frame::new().fill(Color32::TRANSPARENT).inner_margin(Margin::same(18)))
            .show(ctx, |ui| {
                if self.session.is_some() {
                    self.ui_dashboard(ctx, ui);
                } else {
                    self.ui_login(ctx, ui);
                }
            });

        self.draw_toasts(ctx);
    }
}

fn accent_button(text: &str) -> Button<'_> {
    Button::new(RichText::new(text).strong().color(theme::BG_DARKEST))
        .fill(theme::TURQUOISE)
        .stroke(Stroke::new(1.0, theme::TURQUOISE_SOFT))
        .corner_radius(CornerRadius::same(18))
        .min_size(Vec2::new(0.0, 44.0))
}

fn outline_button(text: &str) -> Button<'_> {
    Button::new(RichText::new(text).color(theme::TEXT))
        .fill(theme::PANEL)
        .stroke(Stroke::new(1.0, theme::TURQUOISE))
        .corner_radius(CornerRadius::same(18))
        .min_size(Vec2::new(0.0, 44.0))
}

impl Drop for TokenStudioApp {
    fn drop(&mut self) {
        if let Some(session) = self.session.take() {
            session.logout();
        }
    }
}
