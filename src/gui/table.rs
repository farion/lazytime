use eframe::egui;
use egui_table::{AutoSizeMode, CellInfo, Column, HeaderCellInfo, HeaderRow, Table, TableDelegate};

use super::style;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowAction {
    Select(usize),
    Edit(usize),
    Delete(usize),
    Copy(usize),
    Storno(usize),
}

#[derive(Debug, Clone, Copy)]
pub struct ContextMenuConfig {
    pub edit: bool,
    pub delete: bool,
    pub copy: bool,
    pub storno: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ContextMenuState {
    pub edit_enabled: bool,
    pub delete_enabled: bool,
    pub copy_enabled: bool,
    pub storno_enabled: bool,
}

impl Default for ContextMenuState {
    fn default() -> Self {
        Self {
            edit_enabled: true,
            delete_enabled: true,
            copy_enabled: true,
            storno_enabled: true,
        }
    }
}

impl Default for ContextMenuConfig {
    fn default() -> Self {
        Self {
            edit: true,
            delete: true,
            copy: true,
            storno: false,
        }
    }
}

pub fn render_table(
    ui: &mut egui::Ui,
    id_salt: &str,
    headers: &[&str],
    rows: &[Vec<String>],
    selected: Option<usize>,
    context_menu: Option<ContextMenuConfig>,
    context_menu_state: Option<&[ContextMenuState]>,
    dim_rows: Option<&[bool]>,
) -> Option<RowAction> {
    struct Delegate<'a> {
        headers: &'a [&'a str],
        rows: &'a [Vec<String>],
        selected: Option<usize>,
        dim_rows: Option<&'a [bool]>,
        action: Option<RowAction>,
        context_menu: Option<ContextMenuConfig>,
        context_menu_state: Option<&'a [ContextMenuState]>,
    }

    impl TableDelegate for Delegate<'_> {
        fn header_cell_ui(&mut self, ui: &mut egui::Ui, cell: &HeaderCellInfo) {
            let title = self
                .headers
                .get(cell.col_range.start)
                .copied()
                .unwrap_or_default();
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(
                    style::BUTTON_PAD_X,
                    style::BUTTON_PAD_Y,
                ))
                .show(ui, |ui| {
                    ui.strong(title);
                });
        }

        fn row_ui(&mut self, ui: &mut egui::Ui, row_nr: u64) {
            let row_index = row_nr as usize;
            let state = self
                .context_menu_state
                .and_then(|states| states.get(row_index))
                .copied()
                .unwrap_or_default();
            let row_response = ui.interact(
                ui.max_rect(),
                ui.id().with(("row", row_index)),
                egui::Sense::click(),
            );
            if row_response.double_clicked() && self.context_menu.is_some() && state.edit_enabled {
                self.action = Some(RowAction::Edit(row_index));
            } else if (row_response.clicked() || row_response.secondary_clicked())
                && self.action.is_none()
            {
                self.action = Some(RowAction::Select(row_index));
            }

            if let Some(menu) = self.context_menu {
                row_response.context_menu(|ui| {
                    if menu.edit
                        && ui
                            .add_enabled(state.edit_enabled, egui::Button::new("Edit"))
                            .clicked()
                    {
                        self.action = Some(RowAction::Edit(row_index));
                        ui.close();
                    }
                    if menu.delete
                        && ui
                            .add_enabled(state.delete_enabled, egui::Button::new("Delete"))
                            .clicked()
                    {
                        self.action = Some(RowAction::Delete(row_index));
                        ui.close();
                    }
                    if menu.storno
                        && ui
                            .add_enabled(state.storno_enabled, egui::Button::new("Storno"))
                            .clicked()
                    {
                        self.action = Some(RowAction::Storno(row_index));
                        ui.close();
                    }
                    if menu.copy
                        && ui
                            .add_enabled(state.copy_enabled, egui::Button::new("Copy"))
                            .clicked()
                    {
                        self.action = Some(RowAction::Copy(row_index));
                        ui.close();
                    }
                });
            }

            if self.selected == Some(row_nr as usize) {
                let rect = ui.max_rect();
                ui.painter()
                    .rect_filled(rect, 0.0, ui.visuals().selection.bg_fill);
            }
        }

        fn cell_ui(&mut self, ui: &mut egui::Ui, cell: &CellInfo) {
            let row_nr = cell.row_nr as usize;
            let text = self
                .rows
                .get(row_nr)
                .and_then(|r| r.get(cell.col_nr))
                .cloned()
                .unwrap_or_default();
            let dim_row = self
                .dim_rows
                .and_then(|rows| rows.get(row_nr))
                .copied()
                .unwrap_or(false)
                && self.selected != Some(row_nr);
            let mut text = egui::RichText::new(text);
            if dim_row {
                text = text.color(ui.visuals().weak_text_color());
            }

            let response = egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(
                    style::BUTTON_PAD_X,
                    style::BUTTON_PAD_Y,
                ))
                .show(ui, |ui| {
                    let raw = self
                        .rows
                        .get(row_nr)
                        .and_then(|r| r.get(cell.col_nr))
                        .cloned()
                        .unwrap_or_default();
                    if let Some(color) = super::color::color32_from_hex(&raw) {
                        ui.horizontal(|ui| {
                            let swatch_size = egui::vec2(12.0, 12.0);
                            let (rect, _) =
                                ui.allocate_exact_size(swatch_size, egui::Sense::hover());
                            ui.painter().rect_filled(rect, 2.0, color);
                            ui.painter().rect_stroke(
                                rect,
                                2.0,
                                egui::Stroke::new(
                                    1.0,
                                    ui.visuals().widgets.noninteractive.bg_stroke.color,
                                ),
                                egui::StrokeKind::Outside,
                            );
                            if !raw.is_empty() {
                                ui.label(egui::RichText::new(raw).monospace().color(if dim_row {
                                    ui.visuals().weak_text_color()
                                } else {
                                    ui.visuals().text_color()
                                }));
                            }
                        })
                        .response
                    } else {
                        ui.add(
                            egui::Label::new(text)
                                .selectable(false)
                                .sense(egui::Sense::click()),
                        )
                    }
                })
                .inner;

            if response.double_clicked() && self.context_menu.is_some() {
                self.action = Some(RowAction::Edit(row_nr));
            } else if response.clicked() && self.action.is_none() {
                self.action = Some(RowAction::Select(row_nr));
            }
        }

        fn default_row_height(&self) -> f32 {
            30.0
        }
    }

    let columns: Vec<Column> = headers
        .iter()
        .map(|_| Column::new(160.0).resizable(true))
        .collect();
    let mut delegate = Delegate {
        headers,
        rows,
        selected,
        dim_rows,
        action: None,
        context_menu,
        context_menu_state,
    };

    Table::new()
        .id_salt(id_salt)
        .columns(columns)
        .auto_size_mode(AutoSizeMode::Always)
        .headers(vec![HeaderRow::new(28.0)])
        .num_rows(rows.len() as u64)
        .show(ui, &mut delegate);

    delegate.action
}
