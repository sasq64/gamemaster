use std::io::stdout;
use std::num::NonZeroU32;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
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
    pub style_re: Regex,
    pub stdout_filters: Vec<Regex>,
    /// Commands to send to child stdin
    pub output: Vec<String>,
    /// Accumulated text lines from subprocess (for TextWidget)
    pub text_lines: Vec<Line<'static>>,
    /// Current image size in terminal cells (0 = no image yet)
    pub image_cols: u16,
    pub image_rows: u16,
    /// Kitty image id of the most recently transmitted image
    pub image_id: Option<NonZeroU32>,
    /// Set when a new image needs to be sent via kitty protocol
    pub image_dirty: bool,
    /// Terminal cell pixel dimensions
    pub cell_w: u16,
    pub cell_h: u16,
    pub margin: u16,
}

impl Game {
    pub fn new() -> Self {
        let stdout_filters: Vec<Regex> = [
            r"normal formatting.",
            "^Loading ",
            r"^What now\?",
            r"^>\s*$",
        ]
        .iter()
        .map(|p| Regex::new(p).expect("invalid filter regex"))
        .collect();

        let command_re = Regex::new(r"#\[([^]]*)\]\n?").expect("Invalid regex");
        let style_re = Regex::new(r"\[([^]]*)\]").expect("Invalid regex");

        let size = tcgetwinsize(stdout()).unwrap();
        let cell_w = size.ws_xpixel.checked_div(size.ws_col).unwrap_or(8);
        let cell_h = size.ws_ypixel.checked_div(size.ws_row).unwrap_or(16);
        Game {
            shell: Shell::new(),
            drawer: ImageDrawer::new(),
            prompt_active: false,
            last_output: Instant::now() + Duration::from_millis(500),
            command_re,
            style_re,
            stdout_filters,
            output: vec![],
            text_lines: vec![],
            image_cols: 0,
            image_rows: 0,
            image_id: None,
            image_dirty: false,
            cell_w,
            cell_h,
            margin: 4,
        }
    }

    pub fn tick(&mut self) {
        if !self.prompt_active && Instant::now() - self.last_output > Duration::from_millis(100) {
            self.prompt_active = true;
        }
    }

    pub fn handle_line(&mut self, text: &str) -> Result<()> {
        self.last_output = Instant::now();
        self.prompt_active = false;

        if let Some(caps) = self.command_re.captures(text) {
            if !self.drawer.add_text_command(&caps[1]) {
                return Ok(());
            }
            // "line" commands are incremental drawing ops; skip re-rendering image
            if text.contains("line") {
                return Ok(());
            }
            self.image_dirty = true;
        } else if !self.stdout_filters.iter().any(|re| re.is_match(text)) {
            let mut line = Line::default();
            let mut last_style = Style::default();

            let mut last = 0;
            for cap in self.style_re.captures_iter(text) {
                let m = cap.get(0).unwrap();
                if last < m.start() {
                    let literal = &text[last..m.start()];
                    line.spans
                        .push(Span::styled(literal.to_string(), last_style));
                }
                let cmd = cap.get(1).unwrap().as_str();
                if cmd == "b" {
                    last_style = last_style.bold();
                } else if cmd == "/b" {
                    last_style = last_style.not_bold();
                }
                last = m.end();
            }
            if last < text.len() {
                let literal = &text[last..];
                line.spans
                    .push(Span::styled(literal.to_string(), last_style));
            }
            self.text_lines.push(line);
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
                    let style = Style::new().light_blue();
                    self.text_lines
                        .push(Line::from(format!("> {command}")).style(style));
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
