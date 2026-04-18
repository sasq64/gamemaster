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
    ExecutableCommand, cursor,
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
    style::Color,
    text::{Line, Text},
    widgets::{Paragraph, Widget, Wrap},
};
use regex::Regex;
use std::num::NonZeroU32;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::select;

use crate::image_drawer::ImageDrawer;

// Kitty unicode placeholder character. Cells containing this codepoint are
// replaced by image pixels by the terminal.
const PLACEHOLDER: char = '\u{10EEEE}';

// Fixed image id. Encoded into the cell's foreground color (24-bit RGB) to
// tell kitty which image to render at each placeholder cell.
const IMAGE_ID: u32 = 1;

// Combining marks used to encode row/column indices for placeholder cells.
// Order must match kitty's rowcolumn-diacritics.txt.
const DIACRITICS: [u32; 297] = [
    0x0305, 0x030D, 0x030E, 0x0310, 0x0312, 0x033D, 0x033E, 0x033F, 0x0346, 0x034A, 0x034B, 0x034C,
    0x0350, 0x0351, 0x0352, 0x0357, 0x035B, 0x0363, 0x0364, 0x0365, 0x0366, 0x0367, 0x0368, 0x0369,
    0x036A, 0x036B, 0x036C, 0x036D, 0x036E, 0x036F, 0x0483, 0x0484, 0x0485, 0x0486, 0x0487, 0x0592,
    0x0593, 0x0594, 0x0595, 0x0597, 0x0598, 0x0599, 0x059C, 0x059D, 0x059E, 0x059F, 0x05A0, 0x05A1,
    0x05A8, 0x05A9, 0x05AB, 0x05AC, 0x05AF, 0x05C4, 0x0610, 0x0611, 0x0612, 0x0613, 0x0614, 0x0615,
    0x0616, 0x0617, 0x0657, 0x0658, 0x0659, 0x065A, 0x065B, 0x065D, 0x065E, 0x06D6, 0x06D7, 0x06D8,
    0x06D9, 0x06DA, 0x06DB, 0x06DC, 0x06DF, 0x06E0, 0x06E1, 0x06E2, 0x06E4, 0x06E7, 0x06E8, 0x06EB,
    0x06EC, 0x0730, 0x0732, 0x0733, 0x0735, 0x0736, 0x073A, 0x073D, 0x073F, 0x0740, 0x0741, 0x0743,
    0x0745, 0x0747, 0x0749, 0x074A, 0x07EB, 0x07EC, 0x07ED, 0x07EE, 0x07EF, 0x07F0, 0x07F1, 0x07F3,
    0x0816, 0x0817, 0x0818, 0x0819, 0x081B, 0x081C, 0x081D, 0x081E, 0x081F, 0x0820, 0x0821, 0x0822,
    0x0823, 0x0825, 0x0826, 0x0827, 0x0829, 0x082A, 0x082B, 0x082C, 0x082D, 0x0951, 0x0953, 0x0954,
    0x0F82, 0x0F83, 0x0F86, 0x0F87, 0x135D, 0x135E, 0x135F, 0x17DD, 0x193A, 0x1A17, 0x1A75, 0x1A76,
    0x1A77, 0x1A78, 0x1A79, 0x1A7A, 0x1A7B, 0x1A7C, 0x1B6B, 0x1B6D, 0x1B6E, 0x1B6F, 0x1B70, 0x1B71,
    0x1B72, 0x1B73, 0x1CD0, 0x1CD1, 0x1CD2, 0x1CDA, 0x1CDB, 0x1CE0, 0x1DC0, 0x1DC1, 0x1DC3, 0x1DC4,
    0x1DC5, 0x1DC6, 0x1DC7, 0x1DC8, 0x1DC9, 0x1DCB, 0x1DCC, 0x1DD1, 0x1DD2, 0x1DD3, 0x1DD4, 0x1DD5,
    0x1DD6, 0x1DD7, 0x1DD8, 0x1DD9, 0x1DDA, 0x1DDB, 0x1DDC, 0x1DDD, 0x1DDE, 0x1DDF, 0x1DE0, 0x1DE1,
    0x1DE2, 0x1DE3, 0x1DE4, 0x1DE5, 0x1DE6, 0x1DFE, 0x20D0, 0x20D1, 0x20D4, 0x20D5, 0x20D6, 0x20D7,
    0x20DB, 0x20DC, 0x20E1, 0x20E7, 0x20E9, 0x20F0, 0x2CEF, 0x2CF0, 0x2CF1, 0x2DE0, 0x2DE1, 0x2DE2,
    0x2DE3, 0x2DE4, 0x2DE5, 0x2DE6, 0x2DE7, 0x2DE8, 0x2DE9, 0x2DEA, 0x2DEB, 0x2DEC, 0x2DED, 0x2DEE,
    0x2DEF, 0x2DF0, 0x2DF1, 0x2DF2, 0x2DF3, 0x2DF4, 0x2DF5, 0x2DF6, 0x2DF7, 0x2DF8, 0x2DF9, 0x2DFA,
    0x2DFB, 0x2DFC, 0x2DFD, 0x2DFE, 0x2DFF, 0xA66F, 0xA67C, 0xA67D, 0xA6F0, 0xA6F1, 0xA8E0, 0xA8E1,
    0xA8E2, 0xA8E3, 0xA8E4, 0xA8E5, 0xA8E6, 0xA8E7, 0xA8E8, 0xA8E9, 0xA8EA, 0xA8EB, 0xA8EC, 0xA8ED,
    0xA8EE, 0xA8EF, 0xA8F0, 0xA8F1, 0xAAB0, 0xAAB2, 0xAAB3, 0xAAB7, 0xAAB8, 0xAABE, 0xAABF, 0xAAC1,
    0xFE20, 0xFE21, 0xFE22, 0xFE23, 0xFE24, 0xFE25, 0xFE26, 0x10A0F, 0x10A38, 0x1D185, 0x1D186,
    0x1D187, 0x1D188, 0x1D189, 0x1D1AA, 0x1D1AB, 0x1D1AC, 0x1D1AD, 0x1D242, 0x1D243, 0x1D244,
];

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
        if self.edit_pos == 0 {
            return;
        }
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

struct ImageWidget {
    cols: u16,
    rows: u16,
    image_id: u32,
}

impl Widget for ImageWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let fg = Color::Rgb(
            ((self.image_id >> 16) & 0xFF) as u8,
            ((self.image_id >> 8) & 0xFF) as u8,
            (self.image_id & 0xFF) as u8,
        );
        let max = DIACRITICS.len() as u16;
        let h = self.rows.min(area.height).min(max);
        let w = self.cols.min(area.width).min(max);

        let mut sym = String::with_capacity(12);
        for y in 0..h {
            let row_d = char::from_u32(DIACRITICS[y as usize]).unwrap();
            for x in 0..w {
                let col_d = char::from_u32(DIACRITICS[x as usize]).unwrap();
                sym.clear();
                sym.push(PLACEHOLDER);
                sym.push(row_d);
                sym.push(col_d);
                let cell = &mut buf[(area.x + x, area.y + y)];
                cell.set_symbol(&sym);
                cell.set_fg(fg);
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
    /// Current image size in terminal cells (0 = no image yet)
    image_cols: u16,
    image_rows: u16,
    /// Set when a new image needs to be sent via kitty protocol
    image_dirty: bool,
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

    ImageWidget {
        cols: game.image_cols,
        rows: game.image_rows,
        image_id: IMAGE_ID,
    }
    .render(image_area, frame.buffer_mut());
    TextWidget {
        lines: &game.text_lines,
    }
    .render(text_area, frame.buffer_mut());

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

    // Update image dimensions so the next draw() allocates space and writes
    // the right number of placeholder cells
    game.image_cols = columns;
    game.image_rows = rows;

    let image = Image {
        num_or_id: NumberOrId::Id(NonZeroU32::new(IMAGE_ID).unwrap()),
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
            create_virtual_placement: true,
            ..Default::default()
        },
        placement_id: None,
    };

    let mut out = stdout();
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
        image_dirty: false,
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
