use std::{env, borrow::Cow, path::{Path, PathBuf}, collections::HashMap};
use crate::helpers::{load_aliases, get_alias_file_path};
use crate::globals::ShellState;

pub fn split_command(cmd: &str) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    let mut current: String = String::new();
    let mut in_quotes: bool = false;
    let mut escaped: bool = false;

    for c in cmd.trim().chars() {
        match c {
            '"' if !escaped => {
                in_quotes = !in_quotes;
                if !in_quotes && !current.is_empty() {
                    result.push(std::mem::take(&mut current));
                }
            }
            ' ' if !in_quotes && !escaped => {
                if !current.is_empty() {
                    result.push(std::mem::take(&mut current));
                }
            }
            '\\' if !escaped => {
                escaped = true;
            }
            _ => {
                if escaped {
                    escaped = false;
                }
                current.push(c);
            }
        }
    }

    if !current.is_empty() {
        result.push(current);
    }

    result
}

pub fn expand(state: &mut ShellState, cmd: &str) -> String {
    expand_dots(&expand_env_vars(state, &expand_home(cmd).to_string()))
}

pub fn lim_expand(state: &mut ShellState, cmd: &str) -> String {
    expand_env_vars(state, &expand_home(cmd).to_string())
}

pub fn expand_aliases(cmd_parts: Vec<String>) -> Vec<String>
{
    // Load aliases
    let alias_file_path: PathBuf = get_alias_file_path();
    let aliases: HashMap<String, String> = load_aliases(&alias_file_path);

    // Check for alias and expand if found
    return if let Some(alias_cmd) = aliases.get(&cmd_parts[0]) {
        let mut new_cmd_parts: Vec<String> = alias_cmd.split_whitespace().map(String::from).collect();
        new_cmd_parts.extend_from_slice(&cmd_parts[1..]);
        new_cmd_parts
    } else {
        cmd_parts
    };
}

pub fn expand_dots(cmd: &str) -> String {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let mut result: Vec<String> = Vec::new();

    for part in parts {
        let expanded: String = if part.contains('.') {
            let path: &Path = Path::new(part);
            let mut components: Vec<String> = Vec::new();

            for component in path.components() {
                match component {
                    std::path::Component::CurDir => {
                        components.push(env::current_dir().unwrap_or(PathBuf::from("/")).to_string_lossy().to_string());
                    },
                    std::path::Component::ParentDir => {
                        if !components.is_empty() {
                            components.pop();
                        } else {
                            let mut parent: PathBuf = env::current_dir().unwrap_or(PathBuf::from("/"));
                            parent.pop();
                            components.push(parent.to_string_lossy().into_owned());
                        }
                    },
                    _ => components.push(component.as_os_str().to_string_lossy().into_owned()),
                }
            }

            components.join("/")
        } else {
            part.to_string()
        };

        result.push(expanded);
    }

    result.join(" ")
}

pub fn expand_home(cmd: &str) -> Cow<str> {
    if cmd.contains('~') {
        match dirs::home_dir() {
            Some(home) => {
                let home_str: Cow<'_, str> = home.to_string_lossy();
                Cow::Owned(cmd.replace('~', &home_str))
            }
            None => Cow::Borrowed(cmd),
        }
    } else {
        Cow::Borrowed(cmd)
    }
}

pub fn expand_env_vars(state: &mut ShellState, cmd: &str) -> String {
    let mut result: String = String::new();
    let mut in_var: bool = false;
    let mut var_name: String = String::new();

    for c in cmd.chars() {
        if c == '$' {
            in_var = true;
            var_name.clear();
        } else if in_var {
            if c.is_alphanumeric() || c == '_' {
                var_name.push(c);
            } else {
                in_var = false;
                if let Some(value) = state.get_var(&var_name) {
                    result.push_str(&value);
                } else {
                    result.push('$');
                    result.push_str(&var_name);
                }
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }

    if in_var {
        if let Some(value) = state.get_var(&var_name) {
            result.push_str(&value);
        } else {
            result.push('$');
            result.push_str(&var_name);
        }
    }

    result
}
