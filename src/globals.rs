use std::fs;
use std::path::PathBuf;
use std::io::{self, Write};
use termion::event::Key;
use termion::input::TermRead;
use termion::raw::IntoRawMode;
use termion::cursor::Goto;
use termion::clear;
use std::env;
use std::collections::HashMap;

pub struct ShellState {
    pub hostname: String,
    pub username: String,
    pub history_limit: usize,
    pub ps1_prompt: String,
    pub local_vars: HashMap<String, String>,
}

impl ShellState {
    pub fn set_local_var(&mut self, name: &str, value: &str) {
        self.local_vars.insert(name.to_string(), value.to_string());
    }

    pub fn get_var(&self, name: &str) -> Option<String> {
        self.local_vars.get(name).cloned().or_else(|| env::var(name).ok())
    }
}


pub const NO_RESULT: &str = "";

pub fn get_nash_dir() -> PathBuf {
    let mut path: PathBuf = dirs::home_dir().expect("Unable to get home directory");
    path.push(".nash");
    fs::create_dir_all(&path).expect("Unable to create .nash directory");
    path
}

pub struct GUIMenu {
    pub title: String,
    pub entries: Vec<GUIEntry>,
    selected: usize,
}

pub struct GUIEntry {
    pub name: String,
    pub field_type: String,
    pub value: String,
}

impl GUIEntry {
    pub fn new(name: &str, t: &str, val: &str) -> Self {
        GUIEntry {
            name: name.to_string(),
            field_type: t.to_string(),
            value: val.to_string(),
        }
    }

    fn toggle_bool(&mut self) {
        self.value = if self.value == "true" { "false".to_string() } else { "true".to_string() };
    }
}

impl GUIMenu {
    pub fn new(title: String, entries: Vec<GUIEntry>) -> Self {
        GUIMenu {
            title,
            entries,
            selected: 0,
        }
    }

    pub fn run(&mut self) -> io::Result<()> {
        let stdin: io::Stdin = io::stdin();
        let mut stdout: termion::raw::RawTerminal<io::Stdout> = io::stdout().into_raw_mode()?;

        self.draw(&mut stdout)?;

        for c in stdin.keys() {
            match c? {
                Key::Up => {
                    if self.selected > 0 {
                        self.selected -= 1;
                    }
                }
                Key::Down => {
                    if self.selected < self.entries.len() - 1 {
                        self.selected += 1;
                    }
                }
                Key::Char('\n') => {
                    self.edit_selected_entry(&mut stdout)?;
                }
                Key::Char('q') => break,
                _ => {}
            }
            self.draw(&mut stdout)?;
        }

        Ok(())
    }

    fn draw<W: Write>(&self, stdout: &mut W) -> io::Result<()> {
        write!(stdout, "{}{}", clear::All, Goto(1, 1))?;
        writeln!(stdout, "{}\n", self.title)?;
        
        for (i, entry) in self.entries.iter().enumerate() {
            write!(stdout, "{}", Goto(1, (i as u16) + 3))?;
            if i == self.selected {
                write!(stdout, "> ")?;
            } else {
                write!(stdout, "  ")?;
            }
            writeln!(stdout, "{}: {} ({})", entry.name, entry.value, entry.field_type)?;
        }
        
        write!(stdout, "{}", Goto(1, (self.entries.len() as u16) + 5))?;
        writeln!(stdout, "Use arrow keys to navigate, Enter to edit, 'q' to quit")?;
        stdout.flush()
    }

    fn edit_selected_entry<W: Write>(&mut self, stdout: &mut W) -> io::Result<()> {
        let entries_len: usize = self.entries.len();
        let entry: &mut GUIEntry = &mut self.entries[self.selected];
        match entry.field_type.as_str() {
            "bool" => {
                entry.toggle_bool();
                Ok(())
            },
            "text" | "int" => {
                write!(stdout, "{}", termion::cursor::Show)?;
                write!(stdout, "{}Enter new value: ", Goto(1, (entries_len as u16) + 7))?;
                stdout.flush()?;
                
                let mut new_value: String = String::new();
                let stdin: io::Stdin = io::stdin();

                // This complex method is necessary as stdin().readline(buf) just hangs.
                let mut chars: termion::input::Keys<io::Stdin> = stdin.keys();
                
                loop {
                    if let Some(Ok(key)) = chars.next() {
                        match key {
                            Key::Char('\n') => break,
                            Key::Char(c) => {
                                new_value.push(c);
                                write!(stdout, "{}", c)?;
                                stdout.flush()?;
                            },
                            Key::Backspace => {
                                if !new_value.is_empty() {
                                    new_value.pop();
                                    write!(stdout, "{} {}", termion::cursor::Left(1), termion::cursor::Left(1))?;
                                    stdout.flush()?;
                                }
                            },
                            _ => {}
                        }
                    }
                }
                
                if entry.field_type == "int" && new_value.parse::<i64>().is_ok() {
                    entry.value = new_value;
                } else if entry.field_type == "text" {
                    entry.value = new_value;
                }
                
                write!(stdout, "{}", termion::cursor::Hide)?;
                Ok(())
            }
            _ => Ok(()),
        }
    }     
}

