use std::path::PathBuf;
use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::Context;
use rustyline::Result;
use rustyline::history::History;
use std::fs::DirEntry;
use std::io;
use std::borrow::Cow;
use std::env;
use console::Style;

pub struct AutoCompleter {
    current_dir: PathBuf,
    commands: Vec<String>,
    home_dir: PathBuf,
}

impl AutoCompleter {
    pub fn new(current_dir: PathBuf) -> Self {
        let mut commands: Vec<String> = vec![
            "cd".to_string(),
            "ls".to_string(),
            "cp".to_string(),
            "mv".to_string(),
            "rm".to_string(),
            "mkdir".to_string(),
            "history".to_string(),
            "exit".to_string(),
            "summon".to_string(),
            "alias".to_string(),
            "rmalias".to_string(),
            "help".to_string(),
            "set".to_string(),
            "unset".to_string(),
            "rconf".to_string(),
            "reset".to_string(),
            "settings".to_string(),
            "setprompt".to_string(),
            "export".to_string()
            // More built-in commands here
        ];

        // Index PATH and add external commands
        if let Ok(path) = env::var("PATH") {
            for dir in path.split(":") {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        if let Ok(file_type) = entry.file_type() {
                            if file_type.is_file() {
                                if let Some(file_name) = entry.file_name().to_str() {
                                    commands.push(file_name.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        commands.sort();
        commands.dedup();

        let home_dir: PathBuf = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));

        AutoCompleter { current_dir, commands, home_dir }
    }

    pub fn update_current_dir(&mut self, new_dir: PathBuf) {
        self.current_dir = new_dir;
    }

    fn expand_tilde(&self, path: &str) -> PathBuf {
        if path.starts_with('~') {
            if path == "~" {
                self.home_dir.clone()
            } else if path.starts_with("~/") {
                self.home_dir.join(&path[2..])
            } else {
                PathBuf::from(path)
            }
        } else {
            PathBuf::from(path)
        }
    }
}

impl Completer for AutoCompleter {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Result<(usize, Vec<Pair>)> {
        let (start, word) = extract_word(line, pos);
        let words: Vec<&str> = line[..pos].split_whitespace().collect();

        if words.len() == 1 {
            // Command completion
            let matches: Vec<Pair> = self.commands.iter()
                .filter(|cmd| cmd.starts_with(word))
                .map(|cmd| Pair {
                    display: cmd.clone(),
                    replacement: cmd.clone(),
                })
                .collect();
            return Ok((start, matches));
        }

        // File/directory completion
        let path: PathBuf = self.expand_tilde(word);
        let path: PathBuf = if path.is_absolute() {
            path
        } else {
            self.current_dir.join(path)
        };

        let (dir, prefix) = if path.is_dir() {
            (path.to_path_buf(), "")
        } else {
            (path.parent().unwrap_or(&self.current_dir).to_path_buf(), path.file_name().unwrap_or_default().to_str().unwrap())
        };

        let mut matches: Vec<Pair> = Vec::new();

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.filter_map(|r: io::Result<DirEntry>| r.ok()) {
                let file_name: std::ffi::OsString = entry.file_name();
                let file_name: &str = file_name.to_str().unwrap();
                if file_name.starts_with(prefix) {
                    let mut completed: PathBuf = PathBuf::from(word);
                    completed.set_file_name(file_name);
                    let display: String = completed.to_str().unwrap().to_string();
                    matches.push(Pair {
                        display: display.clone(),
                        replacement: display,
                    });
                }
            }
        }

        Ok((start, matches))
    }
}

fn extract_word(line: &str, pos: usize) -> (usize, &str) {
    let mut start: usize = pos;
    while start > 0 && !line[start - 1..].starts_with(char::is_whitespace) {
        start -= 1;
    }
    (start, &line[start..pos])
}

pub struct LineHighlighter {
    prompt_style: Style,
    command_style: Style,
    arg_style: Style,
    path_style: Style,
}

impl LineHighlighter {
    pub fn new() -> Self {
        LineHighlighter {
            prompt_style: Style::new().cyan(),
            command_style: Style::new().green().bold(),
            arg_style: Style::new().yellow(),
            path_style: Style::new().blue().underlined(),
        }
    }

    fn highlight_command_line(&self, line: &str) -> String {
        let mut result = String::new();
        let parts: Vec<&str> = line.split_whitespace().collect();

        if let Some((first, rest)) = parts.split_first() {
            // Highlight the command
            result.push_str(&self.command_style.apply_to(first).to_string());

            // Highlight arguments and paths
            for part in rest {
                result.push(' ');
                if part.starts_with('-') {
                    result.push_str(&self.arg_style.apply_to(part).to_string());
                } else if part.contains('/') || part.starts_with('~') {
                    result.push_str(&self.path_style.apply_to(part).to_string());
                } else {
                    result.push_str(part);
                }
            }
        } else {
            result.push_str(line);
        }

        result
    }
}

impl Highlighter for LineHighlighter {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(&'s self, prompt: &'p str, _default: bool) -> Cow<'b, str> {
        Cow::Owned(self.prompt_style.apply_to(prompt).to_string())
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Cow::Owned(Style::new().dim().italic().apply_to(hint).to_string())
    }

    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        Cow::Owned(self.highlight_command_line(line))
    }

    fn highlight_char(&self, _line: &str, _pos: usize) -> bool {
        true
    }
}

pub struct CommandHinter {
    history: Vec<String>,
}

impl CommandHinter {
    pub fn new(history: &History) -> Self {
        CommandHinter { 
            history: history.iter().map(|s| s.to_string()).collect() 
        }
    }

    // Modified to accept Vec<String> instead of History
    pub fn update_history(&mut self, history_entries: &Vec<String>) {
        self.history = history_entries.clone();
    }
}

impl Hinter for CommandHinter {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<String> {
        if line.is_empty() || pos < line.len() {
            return None;
        }

        // Search for the most recent command in history that starts with the current input
        for entry in self.history.iter().rev() {
            if entry.starts_with(line) && entry.len() > line.len() {
                return Some(entry[pos..].to_string());
            }
        }

        None
    }
}
