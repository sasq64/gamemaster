use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Flex, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget},
};

use crate::image_widget::ImageWidget;
use crate::{TextWidget, game::Game};

pub fn draw_ui(frame: &mut Frame, game: &mut Game) {
    let border = if game.image_rows > 0 { 2 } else { 0 };

    let [_, main, _] = Layout::horizontal([
        Constraint::Length(game.margin),
        Constraint::Min(1),
        Constraint::Length(game.margin),
    ])
    .areas(frame.area());

    let [status_area, image_row, text_area, prompt_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(game.image_rows + border),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(main);
    let [left_status, right_status] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(status_area);
    let [image_area] = Layout::horizontal([Constraint::Length(game.image_cols)])
        .flex(Flex::Center)
        .areas(image_row);

    //

    let status_style = Style::default()
        .bg(Color::Red)
        .fg(Color::Rgb(0xff, 0xff, 0xff));
    let (left, right) = game.drawer.get_statusbar();

    Paragraph::new(left)
        .style(status_style)
        .alignment(Alignment::Left)
        .render(left_status, frame.buffer_mut());
    Paragraph::new(right)
        .style(status_style)
        .alignment(Alignment::Right)
        .render(right_status, frame.buffer_mut());

    if game.image_rows > 0
        && let Some(image_id) = game.image_id
    {
        let block = Block::bordered();
        let inner = block.inner(image_area);
        block.render(image_area, frame.buffer_mut());
        ImageWidget { image_id }.render(inner, frame.buffer_mut());
    }
    TextWidget {
        lines: &game.text_lines,
    }
    .render(text_area, frame.buffer_mut());

    let text = game.shell.command();
    let prompt_line = Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::LightRed).bold()),
        Span::styled(text, Style::default().light_blue()),
    ]);
    Paragraph::new(prompt_line).render(prompt_area, frame.buffer_mut());

    // Show the real terminal cursor at the input position when waiting for input
    //if game.prompt_active {
    let cursor_col = prompt_area.x + 2 + game.shell.xpos() as u16;
    frame.set_cursor_position((cursor_col, prompt_area.y));
    //}
}
