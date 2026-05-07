use eframe::egui;
use eframe::egui::text::LayoutJob;

pub const BUTTON_PAD_X: i8 = 8;
pub const BUTTON_PAD_Y: i8 = 5;
pub const TEXT_PAD_X: i8 = 12;
pub const TEXT_PAD_Y: i8 = 8;
pub const FIELD_BLOCK_MARGIN: i8 = 5;
pub const DIALOG_MARGIN: i8 = 5;
pub const SIDEBAR_EXPANDED: f32 = 180.0;
pub const SIDEBAR_COLLAPSED: f32 = 56.0;

pub fn text_field_height(ui: &egui::Ui) -> f32 {
    ui.text_style_height(&egui::TextStyle::Body) + (TEXT_PAD_Y as f32 * 2.0) + 2.0
}

pub fn apply_base_style(ctx: &egui::Context) {
    ctx.style_mut(|style| {
        style.spacing.button_padding = egui::vec2(BUTTON_PAD_X as f32, BUTTON_PAD_Y as f32);
        style.spacing.text_edit_width = 240.0;
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    });
}

pub fn padded_text_edit(ui: &mut egui::Ui, value: &mut String) -> egui::Response {
    ui.add(
        egui::TextEdit::singleline(value).margin(egui::Margin::symmetric(TEXT_PAD_X, TEXT_PAD_Y)),
    )
}

pub fn padded_text_edit_sized_validated(
    ui: &mut egui::Ui,
    value: &mut String,
    width: f32,
    error: Option<&str>,
) -> egui::Response {
    let field_height = text_field_height(ui);
    let mut edit =
        egui::TextEdit::singleline(value).margin(egui::Margin::symmetric(TEXT_PAD_X, TEXT_PAD_Y));
    if error.is_some() {
        let palette = validation_palette(ui);
        edit = edit
            .text_color(palette.text)
            .background_color(palette.background);
    }
    ui.add_sized([width, field_height], edit)
}

pub fn padded_text_edit_fill(ui: &mut egui::Ui, value: &mut String) -> egui::Response {
    let field_height = text_field_height(ui);
    ui.add_sized(
        [ui.available_width(), field_height],
        egui::TextEdit::singleline(value).margin(egui::Margin::symmetric(TEXT_PAD_X, TEXT_PAD_Y)),
    )
}

pub struct ValidationPalette {
    pub text: egui::Color32,
    pub background: egui::Color32,
    pub description: egui::Color32,
}

pub fn validation_palette(ui: &egui::Ui) -> ValidationPalette {
    let text = ui.visuals().error_fg_color;
    ValidationPalette {
        text,
        background: text.linear_multiply(0.16),
        description: text.linear_multiply(0.9),
    }
}

pub fn setting_text_row(
    ui: &mut egui::Ui,
    label: &str,
    description: &str,
    label_width: f32,
    value: &mut String,
) {
    setting_row(ui, label, description, label_width, |ui| {
        padded_text_edit_fill(ui, value);
    });
}

pub fn setting_row(
    ui: &mut egui::Ui,
    label: &str,
    description: &str,
    label_width: f32,
    add_field: impl FnOnce(&mut egui::Ui),
) {
    setting_row_with_desc_color(ui, label, description, label_width, None, add_field);
}

pub fn setting_row_with_desc_color(
    ui: &mut egui::Ui,
    label: &str,
    description: &str,
    label_width: f32,
    description_color: Option<egui::Color32>,
    add_field: impl FnOnce(&mut egui::Ui),
) {
    setting_row_with_field_height(
        ui,
        label,
        description,
        label_width,
        description_color,
        text_field_height(ui),
        add_field,
    );
}

pub fn setting_row_with_field_height(
    ui: &mut egui::Ui,
    label: &str,
    description: &str,
    label_width: f32,
    description_color: Option<egui::Color32>,
    field_height: f32,
    add_field: impl FnOnce(&mut egui::Ui),
) {
    let has_description = !description.trim().is_empty();

    // Use a 2-column grid: left is fixed label width, right contains the field widget.
    egui::Grid::new(format!("setting_row_{}", label))
        .num_columns(2)
        .spacing([8.0, 2.0])
        .show(ui, |ui| {
            ui.set_min_height(field_height.max(ui.spacing().interact_size.y));
            // Reserve the fixed label cell and draw the text at the left edge ourselves to guarantee left alignment.
            let (rect, _resp) = ui.allocate_exact_size(
                egui::vec2(label_width, field_height.max(ui.spacing().interact_size.y)),
                egui::Sense::hover(),
            );
            // Layout the label text constrained to the label width and paint it at the cell's left-top.
            let mut job = LayoutJob::default();
            job.append(
                label,
                0.0,
                egui::TextFormat {
                    color: ui.visuals().text_color(),
                    ..Default::default()
                },
            );
            job.wrap.max_width = label_width;
            let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
            let pos = egui::pos2(rect.left(), rect.center().y - (galley.size().y * 0.5));
            ui.painter().add(egui::epaint::TextShape::new(
                pos,
                galley,
                ui.visuals().text_color(),
            ));
            add_field(ui);
            ui.end_row();

            if has_description {
                ui.label("");
                let desc_color =
                    description_color.unwrap_or_else(|| ui.visuals().weak_text_color());
                ui.label(
                    egui::RichText::new(description)
                        .size(14.0)
                        .color(desc_color),
                );
                ui.end_row();
            }
        });

    if has_description {
        ui.add_space(10.0);
    }
}

pub fn icon_label(ui: &egui::Ui, icon: egui_phosphor_icons::Icon, label: &str) -> egui::WidgetText {
    let mut job = LayoutJob::default();
    job.append(
        icon.as_str(),
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::new(18.0, egui::FontFamily::Name("phosphor-regular".into())),
            color: ui.visuals().text_color(),
            ..Default::default()
        },
    );
    if !label.is_empty() {
        job.append(
            "  ",
            0.0,
            egui::TextFormat {
                color: ui.visuals().text_color(),
                ..Default::default()
            },
        );
        job.append(
            label,
            0.0,
            egui::TextFormat {
                color: ui.visuals().text_color(),
                ..Default::default()
            },
        );
    }
    egui::WidgetText::from(job)
}

pub fn field_block<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .inner_margin(egui::Margin::same(FIELD_BLOCK_MARGIN))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add_contents(ui)
        })
        .inner
}

pub fn draw_modal_backdrop(ctx: &egui::Context) {
    let layer = egui::LayerId::new(egui::Order::Middle, egui::Id::new("modal_backdrop"));
    let painter = ctx.layer_painter(layer);
    let rect = ctx.content_rect();
    painter.rect_filled(rect, 0.0, egui::Color32::from_black_alpha(153));

    egui::Area::new(egui::Id::new("modal_blocker"))
        .order(egui::Order::Middle)
        .fixed_pos(rect.min)
        .show(ctx, |ui| {
            ui.allocate_rect(
                egui::Rect::from_min_size(egui::Pos2::ZERO, rect.size()),
                egui::Sense::click_and_drag(),
            );
        });
}
