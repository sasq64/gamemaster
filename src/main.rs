mod draw;
mod image_drawer;

use std::borrow::Cow;
use std::fmt;
use std::io::{Write as _, stdout};
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use rustix::termios::tcgetwinsize;

use crossterm::event::{Event, EventStream, KeyEvent, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{
    QueueableCommand,
    cursor::{self},
    event::KeyCode,
    style::Print,
    terminal::{Clear, ClearType},
};
use futures::StreamExt;
use kittage::ImageDimensions;
use kittage::display::{CursorMovementPolicy, DisplayLocation};
use kittage::{
    NumberOrId, PixelFormat, Verbosity, action::Action as KittyAction, display::DisplayConfig,
    image::Image, medium::Medium,
};
use regex::Regex;
use std::num::NonZeroU32;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::select;

use crate::image_drawer::ImageDrawer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetReverse(pub bool);

impl crossterm::Command for SetReverse {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        if self.0 {
            write!(f, "\x1b[7m")
        } else {
            write!(f, "\x1b[27m")
        }
    }
}

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
        let mut p = self.edit_pos as isize;
        p += delta;
        if p >= 0 && p <= self.cmd.len() as isize {
            self.edit_pos = p as usize;
        }
    }

    fn clear(&mut self) {
        self.cmd.clear();
        self.edit_pos = 0;
    }
}

struct RawModeGuard;

impl RawModeGuard {
    fn new() -> Result<Self> {
        enable_raw_mode().context("failed to enable raw mode")?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = stdout().queue(cursor::Show).unwrap().flush();
    }
}

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

fn print_prompt(shell: &Shell, with_cursor: bool) -> Result<()> {
    let mut out = stdout();
    let (first, cursor, last) = shell.command_line();
    out.queue(cursor::MoveToColumn(0))?
        .queue(Clear(ClearType::UntilNewLine))?
        .queue(Print("> "))?
        .queue(Print(first))?
        .queue(SetReverse(with_cursor))?
        .queue(Print(cursor))?
        .queue(SetReverse(false))?
        .queue(Print(last))?;
    Ok(())
}

struct Game {
    pub shell: Shell,
    pub drawer: ImageDrawer,
    pub prompt_active: bool,
    pub last_output: Instant,
    pub command_re: Regex,
    pub stdout_filters: Vec<Regex>,
    pub output: Vec<String>,
    rows: u16,
    cols: u16,
    cell_w: u16,
    cell_h: u16,
    skip_lines: u16,
}

impl Game {
    fn tick(&mut self) -> Result<()> {
        if !self.prompt_active && Instant::now() - self.last_output > Duration::from_millis(100) {
            self.prompt_active = true;
            print_prompt(&self.shell, true)?;
            stdout().flush()?;
        }
        Ok(())
    }

    fn handle_line(&mut self, line: &str) -> Result<()> {
        self.last_output = Instant::now();
        if self.prompt_active {
            self.prompt_active = false;
            stdout()
                .queue(cursor::MoveToColumn(0))?
                .queue(Clear(ClearType::UntilNewLine))?
                .flush()?;
        }
        if let Some(caps) = self.command_re.captures(line) {
            if !self.drawer.add_text_command(&caps[1]) {
                return Ok(());
            }
            let rgba = self.drawer.get_image()?;
            let (width, height) = self.drawer.get_canvas_size();
            let image = Image {
                num_or_id: NumberOrId::Number(NonZeroU32::MIN),
                format: PixelFormat::Rgba32(ImageDimensions { width, height }, None),
                medium: Medium::Direct {
                    chunk_size: None,
                    data: Cow::Borrowed(&rgba),
                },
            };
            let columns = (width * 3 / self.cell_w as u32) as u16;
            let rows = (height * 3 / self.cell_h as u32) as u16;
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
            action.write_transmit_to(&mut out, Verbosity::Silent)?;
            out.flush()?;
            if rows > self.skip_lines {
                self.skip_lines = rows;
            }
        } else if !self.stdout_filters.iter().any(|re| re.is_match(line)) {
            if self.skip_lines > 0 {
                for _ in 0..self.skip_lines {
                    stdout().queue(Print("\r\n"))?;
                }
                self.skip_lines = 0;
            }
            stdout().queue(Print(line))?.queue(Print("\r\n"))?.flush()?;
        }
        Ok(())
    }

    fn handle_event(&mut self, ev: Event) -> Result<bool> {
        match ev {
            Event::Resize(_, _) => {
                let size = tcgetwinsize(stdout())?;
                self.rows = size.ws_row;
                self.cols = size.ws_col;
                self.cell_w = size.ws_xpixel / self.cols;
                self.cell_h = size.ws_ypixel / self.rows;
            }
            Event::Key(key) => match handle_key(&mut self.shell, &key) {
                Action::Break => {
                    return Ok(true);
                }
                Action::Enter => {
                    let mut command = self.shell.command();
                    print_prompt(&self.shell, false)?;
                    self.shell.clear();
                    stdout().write_all(&[10, 13])?;
                    command.push('\n');
                    self.output.push(command);
                    self.prompt_active = false;
                    self.last_output = Instant::now();
                }
                Action::None => {
                    print_prompt(&self.shell, true)?;
                    stdout().flush()?;
                }
            },
            _ => {}
        }
        Ok(false)
    }
}

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
    // let stderr_pipe = child.stderr.take().context("no stderr handle")?;

    let mut stdout_lines = BufReader::new(stdout_pipe).lines();
    // let mut stderr_lines = BufReader::new(stderr_pipe).lines();

    let stdout_filters: Vec<Regex> = [r"Score: "]
        .iter()
        .map(|p| Regex::new(p).expect("invalid filter regex"))
        .collect();

    let command_re = Regex::new(r"#\[(.*?)\]\n?").expect("Invalid regex");

    let _guard = RawModeGuard::new()?;
    let mut event_stream = EventStream::new();

    let shell = Shell::new();

    let drawer = ImageDrawer::new();

    stdout().queue(cursor::Hide)?.flush()?;

    let last_output = Instant::now() + Duration::from_millis(500);
    let prompt_active = false;

    let mut tick = tokio::time::interval(Duration::from_millis(100));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let size = tcgetwinsize(stdout())?;
    let rows = size.ws_row;
    let cols = size.ws_col;
    let cell_w = size.ws_xpixel / cols;
    let cell_h = size.ws_ypixel / rows;
    let mut game = Game {
        shell,
        drawer,
        prompt_active,
        last_output,
        command_re,
        stdout_filters,
        output: vec![],
        rows,
        cols,
        cell_w,
        cell_h,
        skip_lines: 0,
    };

    loop {
        select! {
            _ = tick.tick() => {
                game.tick()?;
            },
            res = stdout_lines.next_line() => match res? {
                Some(line) => {
                    game.handle_line(&line)?;
                }
                None => break,
            },
            ev = event_stream.next() => {
                let Some(ev) = ev else { break };
                let quit = game.handle_event(ev?)?;
                for out in &game.output {
                    stdout().write_all(&[10, 13])?;
                    child_stdin.write_all(out.as_bytes()).await?;
                    child_stdin.flush().await?;
                }
                game.output.clear();
                if quit {
                    let _ = child.start_kill();
                    break;
                }
            }
            // status = child.wait() => {
            //     let status = status?;
            //      print_raw(&format!("[child exited: {status}]\r\n"));
            //     break;
            // },
        }
    }

    drop(child_stdin);
    let _ = child.wait().await;
    Ok(())
}
