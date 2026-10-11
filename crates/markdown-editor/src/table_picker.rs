// Adapted from Rinx v1.1.0; see resources/NOTICE.md.
//! A Word/Excel-style table size picker: hover over a grid of cells to choose
//! rows × columns, and click to pick. The grid grows as the pointer nears its edge.
use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.TableSizePickerBase = #(TableSizePicker::register_widget(vm))
    mod.widgets.TableSizePicker = set_type_default() do mod.widgets.TableSizePickerBase {
        width: Fit
        height: Fit
        draw_label +: {color: #x555555 text_style: theme.font_regular{font_size: 12}}
    }
}

/// Side of one cell, and the gap between cells, in logical pixels.
const CELL: f64 = 20.0;
const GAP: f64 = 4.0;
const STEP: f64 = CELL + GAP;
/// Rows and columns shown before the pointer grows the grid, and at most.
const MIN_SHOWN: usize = 6;
const MAX_ROWS: usize = 20;
const MAX_COLS: usize = 12;
/// Room below the grid for the "3 行 × 4 列" label, and the least width that fits it.
const LABEL: f64 = 28.0;
const MIN_WIDTH: f64 = 180.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum TableSizePickerAction {
    Picked { rows: usize, cols: usize },
    #[default]
    None,
}

#[derive(Script, ScriptHook, Widget)]
pub struct TableSizePicker {
    #[uid] uid: WidgetUid,
    #[source] source: ScriptObjectRef,
    #[walk] walk: Walk,
    #[layout] layout: Layout,
    #[redraw] #[live] draw_cell: DrawColor,
    #[live] draw_label: DrawText,
    /// The highlighted size as (rows, columns); (0, 0) before the pointer enters.
    #[rust] hover: (usize, usize),
    #[rust] area: Area,
}

impl TableSizePicker {
    /// How many rows and columns the grid shows: one beyond the highlight, so it can grow.
    fn shown(&self) -> (usize, usize) {
        ((self.hover.0 + 1).clamp(MIN_SHOWN, MAX_ROWS), (self.hover.1 + 1).clamp(MIN_SHOWN, MAX_COLS))
    }
    fn set_hover(&mut self, cx: &mut Cx, abs: Vec2d) {
        let (rows, cols) = self.shown();
        let rel = abs - self.area.rect(cx).pos;
        let cell = |v: f64, shown: usize, max: usize| ((v / STEP).floor().max(0.0) as usize + 1).min(shown + 1).min(max);
        let hover = (cell(rel.y, rows, MAX_ROWS), cell(rel.x, cols, MAX_COLS));
        if hover != self.hover {
            self.hover = hover;
            self.redraw(cx);
        }
    }
}

impl Widget for TableSizePicker {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        match event.hits(cx, self.area) {
            Hit::FingerHoverIn(fe) | Hit::FingerHoverOver(fe) => {
                cx.set_cursor(MouseCursor::Hand);
                self.set_hover(cx, fe.abs);
            }
            Hit::FingerDown(fe) => self.set_hover(cx, fe.abs),
            Hit::FingerMove(fe) => self.set_hover(cx, fe.abs),
            Hit::FingerUp(fe) if fe.is_over && self.hover.0 > 0 && self.hover.1 > 0 => {
                let (rows, cols) = self.hover;
                cx.widget_action(self.uid, TableSizePickerAction::Picked { rows, cols });
            }
            _ => {}
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        let (rows, cols) = self.shown();
        let size = dvec2((cols as f64 * STEP - GAP).max(MIN_WIDTH), rows as f64 * STEP - GAP + LABEL);
        let walk = Walk { width: Size::Fixed(size.x), height: Size::Fixed(size.y), ..walk };
        cx.begin_turtle(walk, self.layout);
        let origin = cx.turtle().rect().pos;
        let (border, border_on) = (vec4(0.85, 0.85, 0.85, 1.0), vec4(0.027, 0.757, 0.376, 1.0));
        let (fill, fill_on) = (vec4(1.0, 1.0, 1.0, 1.0), vec4(0.91, 0.973, 0.937, 1.0));
        for r in 0..rows {
            for c in 0..cols {
                let on = r < self.hover.0 && c < self.hover.1;
                let pos = origin + dvec2(c as f64 * STEP, r as f64 * STEP);
                self.draw_cell.color = if on { border_on } else { border };
                self.draw_cell.draw_abs(cx, Rect { pos, size: dvec2(CELL, CELL) });
                self.draw_cell.color = if on { fill_on } else { fill };
                self.draw_cell.draw_abs(cx, Rect { pos: pos + dvec2(1.0, 1.0), size: dvec2(CELL - 2.0, CELL - 2.0) });
            }
        }
        let label = if self.hover.0 > 0 {
            format!("{} rows × {} columns", self.hover.0, self.hover.1)
        } else {
            "Choose the table size".to_owned()
        };
        self.draw_label.draw_abs(cx, origin + dvec2(0.0, rows as f64 * STEP + 6.0), &label);
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }
}

impl TableSizePickerRef {
    /// The size picked by a click in these actions, as (rows, columns).
    pub fn picked(&self, actions: &Actions) -> Option<(usize, usize)> {
        match actions.find_widget_action(self.widget_uid()).cast() {
            TableSizePickerAction::Picked { rows, cols } => Some((rows, cols)),
            TableSizePickerAction::None => None,
        }
    }
    /// Clears the highlight, for the next time the picker opens.
    pub fn reset(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.hover = (0, 0);
            inner.redraw(cx);
        }
    }
}

/// A Markdown table with `rows` rows (the first is the header) and `cols` columns.
pub fn markdown_table(rows: usize, cols: usize, header: impl Fn(usize) -> String) -> String {
    let rows = rows.max(2);
    let cols = cols.max(1);
    let line = |cells: Vec<String>| format!("| {} |", cells.join(" | "));
    let mut out = vec![
        line((1..=cols).map(&header).collect()),
        line(vec!["---".to_owned(); cols]),
    ];
    out.extend((1..rows).map(|_| line(vec![" ".to_owned(); cols])));
    out.join("\n")
}

#[cfg(test)]
mod tests {
    #[test]
    fn tables_have_a_header_row_then_the_rest() {
        let t = super::markdown_table(3, 2, |i| format!("H{i}"));
        assert_eq!(t, "| H1 | H2 |\n| --- | --- |\n|   |   |\n|   |   |");
        // A table needs a header and at least one body row.
        assert_eq!(super::markdown_table(1, 1, |_| "A".into()).lines().count(), 3);
    }
}
