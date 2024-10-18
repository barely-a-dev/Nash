use std::io::{BufRead, BufReader, Write};
use crate::globals::get_nash_dir;
use std::fs::File;
use std::collections::HashMap;
use std::path::PathBuf;
#[derive(Debug)]
pub struct Config
{
    pub rules: HashMap<String, String>,
    pub temp_rules: HashMap<String, String>
}

impl Config
{
    pub fn new() -> Result<Self, std::io::Error> {
        let mut rules: HashMap<String, String> = HashMap::new();
        let config_path: PathBuf = get_nash_dir().join("config");

        if !config_path.exists() {
            // Create the directory if it doesn't exist
            std::fs::create_dir_all(config_path.parent().unwrap())?;
            
            // Create an empty config file
            File::create(&config_path)?;
        }

        // Read and parse the config file
        let file: File = File::open(&config_path)?;
        let reader: BufReader<File> = BufReader::new(file);

        for line in reader.lines() {
            let line: String = line?;
            if let Some((rule, value)) = line.split_once('=') {
                rules.insert(
                    rule.trim().to_string(),
                    value.trim_end_matches(';').trim().to_string(),
                );
            }
        }

        Ok(Config {
            rules,
            temp_rules: HashMap::new(),
        })
    }
    pub fn set_rule(&mut self, rule: &str, value: &str, temp: bool) {
        let rules: &mut HashMap<String, String> = if temp { &mut self.temp_rules } else { &mut self.rules };
        rules.insert(rule.to_string(), value.to_string());
    }

    pub fn get_rule(&self, rule: &str, temp: bool) -> Option<&str> {
        let rules: &HashMap<String, String> = if temp { &self.temp_rules } else { &self.rules };
        rules.get(rule).map(String::as_str)
    }
    pub fn remove_rule(&mut self, rule: &str, temp: bool) -> Option<(String, String)>
    {
        let rules: &mut HashMap<String, String> = if temp { &mut self.temp_rules } else { &mut self.rules };
        rules.remove_entry(rule)
    }
    pub fn save_rules(&self)
    {
        // Save the rules to nash_dir/config
        let mut config_file:File = File::create(get_nash_dir().join("config")).unwrap();
        match config_file.set_len(0)
        {
            Ok(_) => {},
            Err(e) => eprintln!("An error occurred: {}", e)
        }
        let mut formatted_rules: String = String::from("");
        for (key, value) in &self.rules
        {
            formatted_rules.push_str(&format!("{}={};\n", key, value));
        }
        formatted_rules = formatted_rules.to_owned();
        config_file.write_all(formatted_rules.as_bytes()).unwrap();
    }
}

pub fn set_conf_rule(conf: &mut Config, cmd: &Vec<String>) -> String {
    if cmd.len() < 2 {
        return "Usage: set <flag> OR set <option> <value>".to_owned();
    }
    let set: bool;

    match cmd.len() {
        2 => {
            // Command is in "set <flag>" format
            let flag: &str = &cmd[1].trim_start_matches('-');
            match flag
            {
                "e" => conf.set_rule("error", "true", true),
                "d" => conf.set_rule("delete_on_reset", "true", true),
                _ => conf.set_rule(flag, "true", false)
            }
            set = true;
        }
        3 => {
            // Command is in "set <option> <value>" format
            let option: &str = &cmd[1];
            let value: &String = &cmd[2];
            match option
            {
                "error" => conf.set_rule("error", value, true),
                "delete_on_reset" => conf.set_rule("delete_on_reset", value, true),
                _ => conf.set_rule(&option, &value, false)
            }
            set = true;
        }
        4 => {
            // Command is in "set <option> <value>" format
            let option: &str = &cmd[1];
            let value: &String = &cmd[2];
            let temp: &bool = &cmd[3].parse::<bool>().unwrap_or(true);
            match option
            {
                "error" => conf.set_rule("error", value, *temp),
                "delete_on_reset" => conf.set_rule("delete_on_reset", value, *temp),
                _ => conf.set_rule(&option, &value, *temp)
            }
            set = true;
        }
        _ => {
            // Invalid usage
            return "Invalid usage. Use 'set <flag>' or 'set <option> <value>'.".to_owned()
        }
    }
    if set
    {
        conf.save_rules();
    }
    return if set {"Successfully set option".to_owned()} else {"Failed to set option".to_owned()}
}

pub fn unset_conf_rule(conf: &mut Config, cmd: &Vec<String>) -> String
{
    let mut errored: bool = false;
    if cmd.len() == 3
    {
        let temp: bool = match cmd[2].parse::<bool>()
        {
            Ok(b) => b,
            Err(_) => {errored = true; false}
        };
        if errored
        {
            return "You must specify whether the rule is in the temporary or consistent rules.".to_owned();
        }
        conf.remove_rule(&cmd[1], temp).unwrap_or(("".to_owned(), "".to_owned())).1
    }
    else {
        "Usage: unset <option> <temp>".to_owned()
    }
}

pub fn read_conf(conf: &Config, cmd: &Vec<String>) -> String
{
    if cmd.len() == 3
    {
        let temp: bool = match cmd[2].parse::<bool>()
        {
            Ok(b) => b,
            Err(e) => {println!("Could not determine whether searching the temporary list or consistent rules. Assuming consistent. Recieved error: {}", e); false}
        };
        return match conf.get_rule(&cmd[1], temp)
        {
            None => "Rule not set.".to_owned(),
            Some(s) => s.to_owned()
        };
    } else if cmd.len() == 2
    {
        return match conf.get_rule(&cmd[1], false)
        {
            None => format!("Rule not set in consistent, checking temporary.\n{}", match conf.get_rule(&cmd[1], true)
        {
            None => "Rule not set in temporary",
            Some(c) => c
        }).to_owned(),
            Some(s) => s.to_owned()
        };
    }
    else {
        "Usage: rconf <option> [temp(bool)]".to_owned()
    }
}