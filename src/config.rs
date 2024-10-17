use std::io::{BufRead, BufReader, Write};
use crate::get_nash_dir;
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