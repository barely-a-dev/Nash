
use crate::globals::*;
use std::{collections::HashMap, fs, path::PathBuf};

pub fn get_history_file_path() -> PathBuf {
    let mut path: PathBuf = get_nash_dir();
    path.push("history");
    path
}

pub fn get_alias_file_path() -> PathBuf {
    let mut path: PathBuf = get_nash_dir();
    path.push("alias");
    path
}

pub fn load_aliases(path: &PathBuf) -> HashMap<String, String> {
    let mut aliases: HashMap<String, String> = HashMap::new();
    if let Ok(contents) = fs::read_to_string(path) {
        for line in contents.lines() {
            if let Some(pos) = line.find('=') {
                let (name, command) = line.split_at(pos);
                aliases.insert(name.trim().to_string(), command[1..].trim().to_string());
            }
        }
    }
    aliases
}

pub fn save_aliases(path: &PathBuf, aliases: &HashMap<String, String>) {
    let content: String = aliases
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<String>>()
        .join("\n");
    fs::write(path, content).expect("Unable to write alias file");
}
