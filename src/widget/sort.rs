use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    layout::Constraint,
    style::{Color, Style},
    widgets::{Row, Table},
};

use crate::{app::Mode, ui};

use super::{EnumIter, Popup, StatefulTable};

#[derive(Clone)]
pub enum Sort {
    Date,
    Downloads,
    Seeders,
    Leechers,
    Name,
    Category,
}

impl EnumIter<Sort> for Sort {
    fn iter() -> std::slice::Iter<'static, Sort> {
        static SORTS: &'static [Sort] = &[
            Sort::Date,
            Sort::Downloads,
            Sort::Seeders,
            Sort::Leechers,
            Sort::Name,
            Sort::Category,
        ];
        SORTS.iter()
    }
}

impl ToString for Sort {
    fn to_string(&self) -> String {
        match self {
            Sort::Date => "Date".to_owned(),
            Sort::Downloads => "Downloads".to_owned(),
            Sort::Seeders => "Seeders".to_owned(),
            Sort::Leechers => "Leechers".to_owned(),
            Sort::Name => "Name".to_owned(),
            Sort::Category => "Category".to_owned(),
        }
    }
}

pub struct SortPopup {
    pub table: StatefulTable<String>,
    pub selected: Sort,
}

impl Default for SortPopup {
    fn default() -> Self {
        SortPopup {
            table: StatefulTable::with_items(Sort::iter().map(|item| item.to_string()).collect()),
            selected: Sort::Date,
        }
    }
}

impl Popup for SortPopup {
    fn draw(&self, f: &mut ratatui::prelude::Frame) {
        let area = super::centered_rect(30, 8, f.size());
        let items = self.table.items.iter().enumerate().map(|(i, item)| {
            match i == (self.selected.to_owned() as usize) {
                true => Row::new(vec![format!("  {}", item.to_owned())]),
                false => Row::new(vec![format!("   {}", item.to_owned())]),
            }
        });
        let table = Table::new(items, [Constraint::Percentage(100)])
            .block(ui::HI_BLOCK.to_owned().title("Sort"))
            .highlight_style(Style::default().bg(Color::Rgb(60, 60, 60)));
        f.render_stateful_widget(table, area, &mut self.table.state.to_owned());
    }

    fn handle_event(&mut self, app: &mut crate::app::App, e: &crossterm::event::Event) {
        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = e
        {
            match code {
                KeyCode::Esc | KeyCode::Char('s') | KeyCode::Char('q') => {
                    app.mode = Mode::Normal;
                }
                KeyCode::Char('j') => {
                    self.table.next_wrap(1);
                }
                KeyCode::Char('k') => {
                    self.table.next_wrap(-1);
                }
                KeyCode::Char('G') => {
                    self.table.select(self.table.items.len() - 1);
                }
                KeyCode::Char('g') => {
                    self.table.select(0);
                }
                KeyCode::Enter => {
                    if let Some(i) =
                        Sort::iter().nth(self.table.state.selected().unwrap_or_default())
                    {
                        self.selected = i.to_owned();
                        app.mode = Mode::Normal;
                    }
                }
                _ => {}
            }
        }
    }
}
