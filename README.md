# Nash - A Modern Shell Written in Rust

Nash is a simple shell written in Rust, attempting to provide a modern command-line experience with enhanced features and performance.

## Table of Contents

- [Features](#features)
- [Installation](#installation)
  - [Prerequisites](#prerequisites)
  - [Manual Installation](#manual-installation)
  - [Automated Installation](#automated-installation)
  - [Updating](#updating)
- [Usage](#usage)
  - [Command-line Options](#command-line-options)
  - [Built-in Commands](#built-in-commands)
  - [Special Features](#special-features)
- [Development Status](#development-status)
- [Contributing](#contributing)
- [Roadmap](#roadmap)
- [Disclaimer](#disclaimer)
- [License](#license)

## Features

- Modern, fast, and efficient shell implementation in Rust
- Built-in commands for common file system operations
- Command history with searchable interface
- Output redirection and command piping
- Command and file autocompletion
- External command execution support

## Installation

### Prerequisites

- Rust (latest stable version or nightly)
- Git

### Manual Installation

1. Clone the repository:
   ```
   git clone https://github.com/barely-a-dev/Nash.git
   ```
2. Navigate to the Nash directory:
   ```
   cd Nash
   ```
3. Run the installation script:
   ```
   chmod +x ./install.sh && ./install.sh
   ```

### Automated Installation

1. Download the installer: https://github.com/barely-a-dev/Nash/releases/download/v0.0.9.7.5/installer
2. Navigate to the directory containing it:
   ```
   cd ~/Downloads
   ```
3. Make it executable:
   ```
   chmod +x installer
   ```
4. Install nash:
   ```
   ./installer
   ```
5. (optional) Remove the initial installer:
   ```
   rm nbm
   ```

### Updating

To update, simply run one of the following commands:
```
nbm --setver recent
```
```
nbm --setver v<VERSION_NUM>
```
```
nbm --update
```
```
nash --update
```

## Usage

After installation, you can start Nash by typing `nash` in your terminal. The more daring may make nash their default shell by running the following commands:
(Important note: Reminder that Nash is not yet feature rich or compatible with .sh files which expect Bash. Doing this WILL break many applications, or possibly your system. DO THE BELOW AT YOUR OWN RISK.)
1. Add nash to shells:
   ```
   sudo nano /etc/shells
   ```
   and add /usr/bin/nash to the list on a new line.
2. Change shells
   ```
   chsh -s /usr/bin/nash
   ```
   Enter your password and press enter.
3. Log out and log back in or restart. Nash will be your default shell.

### Command-line Options

- `--version`: Display the current version of Nash
- `--update`: Check for updates and install if available (WIP, currently broken)
- `-f, --force`: Force the update operation even if no new version is detected
- `<script>`: Run the specified script file (experimental, use with caution)

### Built-in Commands

- cd <directory>: Change the current directory
- history [--size|s] [--clear|c]: Display command history
- exit: Exit the shell
- summon [-w] <command>: Open an *external* command in a new terminal window (kind of non-functional)
  - -w: Wait for process exit before continuing
- alias <identifier>[=original]: Create an alias for a command
- rmalias <identifier>: Remove an alias for a command
- set <<<option> <value>>/<flag>>: Set a config rule to true or value
- unset <option> <temp(bool)>: Unset a config rule (unimplemented)
- reset: Reset the application, erase if delete_on_reset rule is true
- rconf <option> [temp(bool)]: Read the value of a config rule (unimplemented)
- help: Display a help menu
- settings: Display a simple config menu
- setprompt <format>: Sets the PS1 prompt format
- export: Exports an environment variable to be available to child processes

### Special Features

- Output redirection: `command > file` or `command >> file`
- Piping: `command1 | command2`
- Command chaining: `command1 ; command2`
- Environment variable expansion: `$VAR_NAME`

## Development Status

Nash is currently in early development (v0.0.9.7.5). While it's functional for basic use, many features are still being implemented or improved.

## Contributing

Contributions are welcome! Whether you're fixing bugs, improving documentation, or proposing new features, your help is appreciated. Here's how you can contribute:

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

If you're not sure where to start, check out the [issues](https://github.com/barely-a-dev/Nash/issues) page for open tasks or bugs that need fixing.

## Roadmap

### Checklist

The following features and improvements are planned for future releases:
(Note: "-" means WIP and but possibly partially implemented/unstable, while "/" means WIP but not public. ✔ of course means implemented.)
- [-] Environment variables management system
- [-] Enhanced command auto-completion
- [-] Robust configuration system
- [ ] Quoting and escaping mechanisms
- [✔] Alias command support
- [ ] Scripting capabilities (if, elif, else, for, while, functions, variables)
- [ ] Wildcards and regex support
- [-] Enhanced command-line options
- [✔] Improved argument handling for built-in commands
- [✔] Support for popular, complex commands and text editors (e.g., Nano, Vim)
- [\\] Self-updating capability (instead in NBM, below)
- [✔] Build management system (accessible via nbm command)

### Notable version's planned updates
- 0.1.0: Major bug fix and heavy testing. Will not be released for weeks or months.

- 1.0.0: The point when the project will be comparable to Bash.

# Final goal

## The final goal of nash is:
### [-] Be comparable to or better than Bash in convenience, performance, and overall user-experience.

## Disclaimer

Nash is a work in progress and may not be suitable for production use. Use at your own risk. The project is developed by an amateur, self-taught developer primarily experienced in C#; not Rust, the project's language. As such, the code quality may vary heavily.

## License

[MIT License](LICENSE)

---

For more information, bug reports, or feature requests, please visit the [Nash GitHub repository](https://github.com/barely-a-dev/Nash).
