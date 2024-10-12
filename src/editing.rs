use std::path::PathBuf;
use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::Context;
use rustyline::Result;
use std::fs::DirEntry;
use std::io;
use std::borrow::Cow;
use std::env;

pub struct AutoCompleter {
    current_dir: PathBuf,
    commands: Vec<String>,
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
            "rmalias".to_string()
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

        AutoCompleter { current_dir, commands }
    }

    pub fn update_current_dir(&mut self, new_dir: PathBuf) {
        self.current_dir = new_dir;
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
        let path: PathBuf = if word.starts_with('/') {
            PathBuf::from(word)
        } else {
            self.current_dir.join(word)
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

pub struct LineHighlighter;

impl LineHighlighter {
    pub fn new() -> Self {
        LineHighlighter
    }
}

impl Highlighter for LineHighlighter {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(&'s self, prompt: &'p str, _default: bool) -> Cow<'b, str> {
        Cow::Borrowed(prompt)
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Cow::Borrowed(hint)
    }

    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        Cow::Borrowed(line)
    }

    fn highlight_char(&self, _line: &str, _pos: usize) -> bool {
        false
    }
}

pub struct CommandHinter;

impl CommandHinter {
    pub fn new() -> Self {
        CommandHinter
    }
}

impl Hinter for CommandHinter {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<String> {
        if line.is_empty() || pos < line.len() {
            return None;
        }

        // Split the line by ;, >, and | and get the last part
        let last_part = line.split(&[';', '>', '|'][..]).last()?;
        let trimmed_last_part = last_part.trim();

        let command = trimmed_last_part.split_whitespace().next()?;
        match command {
            "cd" => Some(" <directory>".to_string()),
            "ls" => Some(" [directory]".to_string()),
            "cp" => Some(" <source> <destination>".to_string()),
            "mv" => Some(" <source> <destination>".to_string()),
            "rm" => Some(" <file>".to_string()),
            "mkdir" => Some(" <directory>".to_string()),
            "summon" => Some(" <command>".to_string()),
            "alias" => Some(" <identifier>[=<command>]".to_string()),
            "rmalias" => Some(" <identifier>".to_string()),
            _ => None,
        }
    }
}
