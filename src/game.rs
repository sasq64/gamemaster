use std::io::stdout;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use regex::Regex;
use rustix::termios::tcgetwinsize;

use crate::Shell;
use crate::image_drawer::ImageDrawer;

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

pub struct Game {
    pub shell: Shell,
    pub drawer: ImageDrawer,
    pub prompt_active: bool,
    pub last_output: Instant,
    pub command_re: Regex,
    pub stdout_filters: Vec<Regex>,
    /// Commands to send to child stdin
    pub output: Vec<String>,
    /// Accumulated text lines from subprocess (for TextWidget)
    pub text_lines: Vec<String>,
    /// Current image size in terminal cells (0 = no image yet)
    pub image_cols: u16,
    pub image_rows: u16,
    /// Set when a new image needs to be sent via kitty protocol
    pub image_dirty: bool,
    /// Terminal cell pixel dimensions
    pub cell_w: u16,
    pub cell_h: u16,
}

impl Game {
    pub fn tick(&mut self) {
        if !self.prompt_active && Instant::now() - self.last_output > Duration::from_millis(100) {
            self.prompt_active = true;
        }
    }

    pub fn handle_line(&mut self, line: &str) -> Result<()> {
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

    pub fn handle_event(&mut self, ev: Event) -> Result<bool> {
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
