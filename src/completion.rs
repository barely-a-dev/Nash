use std::path::PathBuf;
use rustyline::completion::{Completer, Pair};
use rustyline::Context;
use rustyline::Result;
use std::fs::DirEntry;
use std::io;
use std::env;

pub struct AutoCompleter {
    current_dir: PathBuf,
    commands: Vec<String>,
}

impl AutoCompleter {
    pub fn new() -> Self {
        let current_dir: PathBuf = env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let commands: Vec<String> = vec![
            "cd".to_string(),
            "ls".to_string(),
            "cp".to_string(),
            "mv".to_string(),
            "rm".to_string(),
            "mkdir".to_string(),
            "history".to_string(),
            "exit".to_string(),
            "summon".to_string(),
            // More built-in commands here
        ];
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
