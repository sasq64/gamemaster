mod draw;
mod image_drawer;

use std::borrow::Cow;
use std::io::{Write as _, stdout};
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use rustix::termios::tcgetwinsize;

use crossterm::event::{Event, EventStream, KeyEvent, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{
    ExecutableCommand, QueueableCommand,
    cursor,
    event::KeyCode,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use kittage::ImageDimensions;
use kittage::display::{CursorMovementPolicy, DisplayLocation};
use kittage::{
    NumberOrId, PixelFormat, Verbosity, action::Action as KittyAction, display::DisplayConfig,
    image::Image, medium::Medium,
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    text::{Line, Text},
    widgets::{Paragraph, Widget, Wrap},
};
use regex::Regex;
use std::num::NonZeroU32;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::select;

use crate::image_drawer::ImageDrawer;

// --- Shell ---

pub struct Shell {
    cmd: Vec<char>,
    edit_pos: usize,
}

impl Shell {
    fn new() -> Self {
        Self { cmd: Vec::new(), edit_pos: 0 }
    }

    fn command(&self) -> String {
        self.cmd.iter().collect()
    }

    fn command_line(&self) -> (String, char, String) {
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

    fn insert(&mut self, c: char) {
        self.cmd.insert(self.edit_pos, c);
        self.edit_pos += 1;
    }

    fn del(&mut self) {
        if self.edit_pos == 0 { return; }
        self.edit_pos -= 1;
        self.cmd.remove(self.edit_pos);
    }

    fn go(&mut self, delta: isize) {
        let p = self.edit_pos as isize + delta;
        if p >= 0 && p <= self.cmd.len() as isize {
            self.edit_pos = p as usize;
        }
    }

    fn clear(&mut self) {
        self.cmd.clear();
        self.edit_pos = 0;
    }
}

// --- Cleanup guard ---

struct Guard;

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = stdout().execute(LeaveAlternateScreen);
        let _ = stdout().execute(cursor::Show);
    }
}

// --- Key handling ---

enum Action {
    None,
    Break,
    Enter,
}

fn handle_key(shell: &mut Shell, key: &KeyEvent) -> Action {
    let m = key.modifiers;
    match key.code {
        KeyCode::Enter => return Action::Enter,
        KeyCode::Left => shell.go(-1),
        KeyCode::Right => shell.go(1),
        KeyCode::Backspace => shell.del(),
        KeyCode::Char('c') if m == KeyModifiers::CONTROL => return Action::Break,
        KeyCode::Char(c) if m == KeyModifiers::NONE || m == KeyModifiers::SHIFT => {
            shell.insert(c);
        }
        _ => {}
    }
    Action::None
}

// --- Widgets ---

struct ImageWidget;

impl Widget for ImageWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Clear all cells in the image area so ratatui's diff engine
        // writes spaces over any previous content, giving kitty a blank canvas.
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)].reset();
            }
        }
    }
}

struct TextWidget<'a> {
    lines: &'a [String],
}

impl Widget for TextWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let width = area.width as usize;
        let height = area.height as usize;

        // Walk backwards through lines, accumulating visual rows, until the
        // area is full. This ensures the last line is always at the bottom.
        let mut visual_rows = 0usize;
        let mut start = self.lines.len();
        for line in self.lines.iter().rev() {
            let rows = visual_row_count(line, width);
            if visual_rows + rows > height {
                break;
            }
            visual_rows += rows;
            start -= 1;
        }

        // Pad the top with empty lines so content is bottom-aligned.
        let padding = height.saturating_sub(visual_rows);
        let mut display: Vec<Line> = (0..padding).map(|_| Line::raw("")).collect();
        display.extend(self.lines[start..].iter().map(|l| Line::raw(l.as_str())));

        Paragraph::new(Text::from(display))
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }
}

/// Estimate the number of visual rows a line occupies when word-wrapped to
/// `width` columns. Uses character count as a close approximation.
fn visual_row_count(line: &str, width: usize) -> usize {
    if width == 0 {
        return 1;
    }
    let chars = line.chars().count();
    if chars == 0 { 1 } else { chars.div_ceil(width) }
}

// --- Game state ---

struct Game {
    shell: Shell,
    drawer: ImageDrawer,
    prompt_active: bool,
    last_output: Instant,
    command_re: Regex,
    stdout_filters: Vec<Regex>,
    /// Commands to send to child stdin
    output: Vec<String>,
    /// Accumulated text lines from subprocess (for TextWidget)
    text_lines: Vec<String>,
    /// Current image height in terminal cells (0 = no image yet)
    image_rows: u16,
    /// Set when a new image needs to be sent via kitty protocol
    image_dirty: bool,
    /// Saved from last layout pass — where to draw the kitty image
    image_area: Option<Rect>,
    /// Terminal cell pixel dimensions
    cell_w: u16,
    cell_h: u16,
}

impl Game {
    fn tick(&mut self) {
        if !self.prompt_active && Instant::now() - self.last_output > Duration::from_millis(100) {
            self.prompt_active = true;
        }
    }

    fn handle_line(&mut self, line: &str) -> Result<()> {
        self.last_output = Instant::now();
        self.prompt_active = false;

        if let Some(caps) = self.command_re.captures(line) {
            if !self.drawer.add_text_command(&caps[1]) {
                return Ok(());
            }
            // "line" commands are incremental drawing ops; skip re-rendering image
            if line.contains("line") {
                return Ok(());
            }
            self.image_dirty = true;
        } else if !self.stdout_filters.iter().any(|re| re.is_match(line)) {
            self.text_lines.push(line.to_string());
        }
        Ok(())
    }

    fn handle_event(&mut self, ev: Event) -> Result<bool> {
        match ev {
            Event::Resize(cols, rows) => {
                let size = tcgetwinsize(stdout())?;
                let c = if cols > 0 { cols } else { size.ws_col };
                let r = if rows > 0 { rows } else { size.ws_row };
                self.cell_w = size.ws_xpixel / c;
                self.cell_h = size.ws_ypixel / r;
                self.image_dirty = true;
            }
            Event::Key(key) => match handle_key(&mut self.shell, &key) {
                Action::Break => return Ok(true),
                Action::Enter => {
                    let mut command = self.shell.command();
                    self.shell.clear();
                    command.push('\n');
                    self.output.push(command);
                    self.prompt_active = false;
                    self.last_output = Instant::now();
                }
                Action::None => {}
            },
            _ => {}
        }
        Ok(false)
    }
}

// --- Rendering ---

fn draw_ui(frame: &mut Frame, game: &mut Game) {
    let [image_area, text_area, prompt_area] = Layout::vertical([
        Constraint::Length(game.image_rows),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    game.image_area = if game.image_rows > 0 { Some(image_area) } else { None };

    ImageWidget.render(image_area, frame.buffer_mut());
    TextWidget { lines: &game.text_lines }.render(text_area, frame.buffer_mut());

    let (before, cursor_ch, after) = game.shell.command_line();
    let prompt_line = Line::from(format!("> {before}{cursor_ch}{after}"));
    Paragraph::new(prompt_line).render(prompt_area, frame.buffer_mut());

    // Show the real terminal cursor at the input position when waiting for input
    if game.prompt_active {
        let cursor_col = prompt_area.x + 2 + before.chars().count() as u16;
        frame.set_cursor_position((cursor_col, prompt_area.y));
    }
}

fn send_kitty_image(game: &mut Game) -> Result<()> {
    let w = game.drawer.get_canvas_size().0;
    let mut scale = 1u32;
    while (w * scale) < 60 * game.cell_w as u32 {
        scale += 1;
    }

    let rgba = game.drawer.get_scaled_image_fir(scale)?;
    let width = rgba.width;
    let height = rgba.height;
    let columns = (width / game.cell_w as u32) as u16;
    let rows = (height / game.cell_h as u32) as u16;

    // Update image_rows so the next draw() allocates the right space
    game.image_rows = rows;

    let image = Image {
        num_or_id: NumberOrId::Number(NonZeroU32::MIN),
        format: PixelFormat::Rgba32(ImageDimensions { width, height }, None),
        medium: Medium::Direct {
            chunk_size: None,
            data: Cow::Borrowed(&rgba.rgba),
        },
    };
    let action = KittyAction::TransmitAndDisplay {
        image,
        config: DisplayConfig {
            location: DisplayLocation {
                columns,
                rows,
                ..Default::default()
            },
            cursor_movement: CursorMovementPolicy::DontMove,
            ..Default::default()
        },
        placement_id: None,
    };

    let mut out = stdout();
    // Hide cursor during kitty send to avoid visible flicker
    out.queue(cursor::Hide)?;
    // Position cursor at the top-left of the reserved image area
    if let Some(area) = game.image_area {
        out.queue(cursor::MoveTo(area.x, area.y))?;
    } else {
        out.queue(cursor::MoveTo(0, 0))?;
    }
    action.write_transmit_to(&mut out, Verbosity::Silent)?;
    out.flush()?;
    Ok(())
}

// --- Main ---

#[tokio::main]
async fn main() -> Result<()> {
    let mut argv = std::env::args().skip(1);
    let program = match argv.next() {
        Some(p) => p,
        None => bail!("usage: gamemaster <program> [args...]"),
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

    let stdout_filters: Vec<Regex> = [r"Score: "]
        .iter()
        .map(|p| Regex::new(p).expect("invalid filter regex"))
        .collect();

    let command_re = Regex::new(r"#\[(.*?)\]\n?").expect("Invalid regex");

    // Setup terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;

    let _guard = Guard;

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
        image_rows: 0,
        image_dirty: false,
        image_area: None,
        cell_w,
        cell_h,
    };

    let mut event_stream = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let mut should_quit = false;

    loop {
        terminal.draw(|frame| draw_ui(frame, &mut game))?;

        if game.image_dirty {
            send_kitty_image(&mut game)?;
            game.image_dirty = false;
            // Redraw immediately so layout reflects the new image_rows
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
                Some(line) => game.handle_line(&line)?,
                None => break,
            },
            ev = event_stream.next() => {
                let Some(ev) = ev else { break };
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
