use std::collections::HashMap;
pub fn parse_args(args: &[String]) -> (Vec<String>, HashMap<String, Option<String>>) {
    let value_args: Vec<String> = Vec::from(["".to_owned()]);
    let mut parsed_args: HashMap<String, Option<String>> = HashMap::new();
    let mut non_flag_args: Vec<String> = Vec::new();
    let mut i: usize = 1; // Start from 1 to skip the program name

    while i < args.len() {
        let arg: &String = &args[i];
        
        if arg.starts_with("--") {
            // Long option
            let option: String = arg[2..].to_string();
            // Don't accept any arguments past a blank long option
            if arg == "--"
            {
                return (non_flag_args, parsed_args);
            }
            if i + 1 < args.len() && !args[i + 1].starts_with('-') && value_args.contains(&args[i + 1]) {
                // Option with value
                parsed_args.insert(option, Some(args[i + 1].clone()));
                i += 2;
            } else {
                // Flag option
                parsed_args.insert(option, None);
                i += 1;
            }
        } else if arg.starts_with('-') {
            // Short option
            let options: Vec<char> = arg[1..].chars().collect();
            for (j, opt) in options.iter().enumerate() {
                let option: String = opt.to_string();
                if j == options.len() - 1 && i + 1 < args.len() && !args[i + 1].starts_with('-') && value_args.contains(&args[i + 1]) {
                    // Last option in group with value
                    parsed_args.insert(option, Some(args[i + 1].clone()));
                    i += 1;
                } else {
                    // Flag option
                    parsed_args.insert(option, None);
                }
            }
            i += 1;
        } else {
            // Non-option argument
            non_flag_args.push(arg.clone());
            i += 1;
        }
    }

    (non_flag_args, parsed_args)
}

// This isn't pointless at all
pub fn parse_arg_vec(args: &Vec<String>) -> (Vec<String>, HashMap<String, Option<String>>)
{
    let arg_array: &[String] = args.as_slice();
    
    return parse_args(arg_array);
}