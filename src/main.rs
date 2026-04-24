mod draw;
mod game;
mod image_drawer;
mod image_widget;

use std::io::{Write, stdout};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use ratatui::style::Stylize;
use ratatui::text::Span;
use rustix::termios::tcgetwinsize;

use crossterm::event::EventStream;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{
    ExecutableCommand, cursor,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::{Alignment, Constraint, Flex, Layout, Rect},
    style::{Color, Style},
    text::{Line, Text},
    widgets::{Block, Paragraph, Widget, Wrap},
};
use regex::Regex;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::select;

use crate::game::Game;
use crate::image_drawer::ImageDrawer;
use crate::image_widget::ImageWidget;

#[derive(Default, Parser, Debug, Clone)]
#[command(version, about, author, long_about = None)]
pub struct Args {
    game: PathBuf,
}

// --- Shell ---

pub struct Shell {
    cmd: Vec<char>,
    edit_pos: usize,
}

impl Shell {
    fn new() -> Self {
        Self {
            cmd: Vec::new(),
            edit_pos: 0,
        }
    }

    pub fn command(&self) -> String {
        self.cmd.iter().collect()
    }

    pub fn xpos(&self) -> usize {
        self.edit_pos
    }

    pub fn command_line(&self) -> (String, char, String) {
        let at_end = self.edit_pos == self.cmd.len();
        (
            self.cmd[..self.edit_pos].iter().collect(),
            if at_end { ' ' } else { self.cmd[self.edit_pos] },
            if at_end {
                String::new()
            } else {
                self.cmd[self.edit_pos + 1..].iter().collect()
            },
        )
    }

    pub fn insert(&mut self, c: char) {
        self.cmd.insert(self.edit_pos, c);
        self.edit_pos += 1;
    }

    pub fn del(&mut self) {
        if self.edit_pos == 0 {
            return;
        }
        self.edit_pos -= 1;
        self.cmd.remove(self.edit_pos);
    }

    pub fn go(&mut self, delta: isize) {
        let p = self.edit_pos as isize + delta;
        if p >= 0 && p <= self.cmd.len() as isize {
            self.edit_pos = p as usize;
        }
    }

    pub fn clear(&mut self) {
        self.cmd.clear();
        self.edit_pos = 0;
    }
}

// --- Cleanup guard ---

static TERMINAL_RESTORED: AtomicBool = AtomicBool::new(false);

struct Guard;

impl Drop for Guard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

fn restore_terminal() {
    if TERMINAL_RESTORED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = disable_raw_mode();
    let _ = stdout().execute(LeaveAlternateScreen);
    let _ = stdout().execute(cursor::Show);
    let _ = stdout().flush();
}

/// Install a panic hook that restores the terminal and then writes the panic
/// message directly to stdout (same stream as the terminal commands, flushed
/// explicitly) so the error is visible instead of being lost to the
/// alt-screen / raw-mode state.
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        restore_terminal();
        let mut out = stdout().lock();
        let _ = writeln!(out, "\n{info}");
        let _ = out.flush();
    }));
}

// --- Widgets ---

struct TextWidget<'a> {
    lines: &'a [String],
}

impl Widget for TextWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let height = area.height as usize;

        let display: Vec<Line> = self.lines.iter().map(|l| Line::raw(l.as_str())).collect();
        let paragraph = Paragraph::new(Text::from(display)).wrap(Wrap { trim: false });

        let total_rows = paragraph.line_count(area.width);
        let scroll = total_rows.saturating_sub(height).min(u16::MAX as usize) as u16;

        paragraph.scroll((scroll, 0)).render(area, buf);
    }
}

// --- Rendering ---

fn draw_ui(frame: &mut Frame, game: &mut Game) {
    let border = if game.image_rows > 0 { 2 } else { 0 };

    // Layout
    let [status_area, image_row, text_area, prompt_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(game.image_rows + border),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    let [left_status, right_status] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(status_area);
    let [image_area] = Layout::horizontal([Constraint::Length(game.image_cols)])
        .flex(Flex::Center)
        .areas(image_row);

    let [_, text_area, _] = Layout::horizontal([
        Constraint::Length(4),
        Constraint::Min(1),
        Constraint::Length(4),
    ])
    .areas(text_area);

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
        Span::raw(text),
    ]);
    Paragraph::new(prompt_line).render(prompt_area, frame.buffer_mut());

    // Show the real terminal cursor at the input position when waiting for input
    //if game.prompt_active {
    let cursor_col = prompt_area.x + 2 + game.shell.xpos() as u16;
    frame.set_cursor_position((cursor_col, prompt_area.y));
    //}
}

fn send_kitty_image(game: &mut Game) -> Result<()> {
    //let w = game.drawer.get_canvas_size().0;
    let scale = 4u32;
    //while (w * scale) < 60 * game.cell_w as u32 {
    //    scale += 1;
    //}

    let rgba = game.drawer.get_scaled_image(scale)?;
    let width = rgba.width();
    let height = rgba.height();
    let columns = width / game.cell_w as u32;
    let cols = 80;
    let rows = (height * cols / (game.cell_h as u32 * columns)) as u16;

    let id = ImageWidget::create_image(&rgba, game.image_id)?;

    // Update image dimensions so the next draw() allocates space and writes
    // the right number of placeholder cells
    game.image_cols = cols as u16;
    game.image_rows = rows;
    game.image_id = Some(id);
    Ok(())
}

// --- Main ---

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let mut argv = std::env::args().skip(1);
    let program = match argv.next() {
        Some(p) => p,
        None => anyhow::bail!("usage: gamemaster <program> [args...]"),
    };
    let args: Vec<String> = argv.collect();

    let mut child = tokio::process::Command::new(&program)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn {program}"))?;

    let mut child_stdin = child.stdin.take().context("no stdin handle")?;
    let stdout_pipe = child.stdout.take().context("no stdout handle")?;

    let mut stdout_lines = BufReader::new(stdout_pipe).lines();

    let stdout_filters: Vec<Regex> = [r"normal formatting.", "^Loading ", r"^>\s*$"]
        .iter()
        .map(|p| Regex::new(p).expect("invalid filter regex"))
        .collect();

    let command_re = Regex::new(r"#\[(.*?)\]\n?").expect("Invalid regex");

    // Setup terminal
    install_panic_hook();
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;

    let _guard = Guard {};

    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let size = tcgetwinsize(stdout())?;
    let rows = size.ws_row;
    let cols = size.ws_col;
    let cell_w = if cols > 0 { size.ws_xpixel / cols } else { 8 };
    let cell_h = if rows > 0 { size.ws_ypixel / rows } else { 16 };

    let mut game = Game {
        shell: Shell::new(),
        drawer: ImageDrawer::new(),
        prompt_active: false,
        last_output: Instant::now() + Duration::from_millis(500),
        command_re,
        stdout_filters,
        output: vec![],
        text_lines: vec![],
        image_cols: 0,
        image_rows: 0,
        image_id: None,
        image_dirty: false,
        cell_w,
        cell_h,
    };

    let mut event_stream = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let mut should_quit = false;

    let mut frame_dirty = true;

    loop {
        if game.image_dirty {
            send_kitty_image(&mut game)?;
            game.image_dirty = false;
            frame_dirty = true;
        }
        if frame_dirty {
            frame_dirty = false;
            terminal.draw(|frame| draw_ui(frame, &mut game))?;
        }

        // Drain pending child stdin writes
        for cmd in game.output.drain(..) {
            child_stdin.write_all(cmd.as_bytes()).await?;
            child_stdin.flush().await?;
        }

        if should_quit {
            let _ = child.start_kill();
            break;
        }

        select! {
            _ = tick.tick() => {
                game.tick();
            },
            res = stdout_lines.next_line() => match res? {
                Some(line) => {
                    frame_dirty = true;
                    game.handle_line(&line)?;
                }
                None => break,
            },
            ev = event_stream.next() => {
                let Some(ev) = ev else { break };
                frame_dirty = true;
                if game.handle_event(ev?)? {
                    should_quit = true;
                }
            }
        }
    }

    drop(child_stdin);
    let _ = child.wait().await;
    Ok(())
}
