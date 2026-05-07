use eframe::egui;
use egui_table::{AutoSizeMode, CellInfo, Column, HeaderCellInfo, HeaderRow, Table, TableDelegate};

use super::style;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowAction {
    Select(usize),
    Edit(usize),
    Delete(usize),
    Copy(usize),
}

pub fn render_table(
    ui: &mut egui::Ui,
    id_salt: &str,
    headers: &[&str],
    rows: &[Vec<String>],
    selected: Option<usize>,
    with_context_menu: bool,
    dim_rows: Option<&[bool]>,
) -> Option<RowAction> {
    struct Delegate<'a> {
        headers: &'a [&'a str],
        rows: &'a [Vec<String>],
        selected: Option<usize>,
        dim_rows: Option<&'a [bool]>,
        action: Option<RowAction>,
        with_context_menu: bool,
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
            let row_response = ui.interact(
                ui.max_rect(),
                ui.id().with(("row", row_index)),
                egui::Sense::click(),
            );
            if row_response.double_clicked() && self.with_context_menu {
                self.action = Some(RowAction::Edit(row_index));
            } else if row_response.clicked() && self.action.is_none() {
                self.action = Some(RowAction::Select(row_index));
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
                    ui.add(
                        egui::Label::new(text)
                            .selectable(false)
                            .sense(egui::Sense::click()),
                    )
                })
                .inner;

            if response.double_clicked() && self.with_context_menu {
                self.action = Some(RowAction::Edit(row_nr));
            } else if response.clicked() && self.action.is_none() {
                self.action = Some(RowAction::Select(row_nr));
            }

            if self.with_context_menu {
                response.context_menu(|ui| {
                    if ui.button("Edit").clicked() {
                        self.action = Some(RowAction::Edit(row_nr));
                        ui.close();
                    }
                    if ui.button("Delete").clicked() {
                        self.action = Some(RowAction::Delete(row_nr));
                        ui.close();
                    }
                    if ui.button("Copy").clicked() {
                        self.action = Some(RowAction::Copy(row_nr));
                        ui.close();
                    }
                });
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
        with_context_menu,
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
