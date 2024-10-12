# Nash - A Modern Shell Written in Rust

Nash is a simple shell written in Rust, designed to provide a modern command-line experience with enhanced features and performance.

## Table of Contents

- [Features](#features)
- [Installation](#installation)
  - [Prerequisites](#prerequisites)
  - [Manual Installation](#manual-installation)
  - [Automated Installation](#automated-installation)
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
- Self-updating capability

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

1. Download the latest release from [https://github.com/barely-a-dev/Nash/releases](https://github.com/barely-a-dev/Nash/releases)
2. Run it with force update flags:
   ```
   cd /home/<username>/Downloads && chmod +x nash && nash --update -f
   ```

## Usage

After installation, you can start Nash by typing `nash` in your terminal.

### Command-line Options

- `--version`: Display the current version of Nash
- `--update`: Check for updates and install if available
- `-f, --force`: Force the update operation even if no new version is detected
- `<script>`: Run the specified script file (experimental, use with caution)

### Built-in Commands

- `cd [directory]`: Change the current directory
- `ls [directory]`: List contents of a directory
- `cp <source> <destination>`: Copy files or directories
- `mv <source> <destination>`: Move files or directories
- `rm <file>`: Remove a file
- `mkdir <directory>`: Create a new directory
- `history`: Display command history
- `exit`: Exit the shell
- `summon <command>`: Open an external command in a new terminal window

### Special Features

- Output redirection: `command > file` or `command >> file`
- Piping: `command1 | command2`
- Command chaining: `command1 ; command2`
- Environment variable expansion: `$VAR_NAME`

## Development Status

Nash is currently in early development (v0.0.7). While it's functional for basic use, many features are still being implemented or improved.

## Contributing

Contributions are welcome! Whether you're fixing bugs, improving documentation, or proposing new features, your help is appreciated. Here's how you can contribute:

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

If you're not sure where to start, check out the [issues](https://github.com/barely-a-dev/Nash/issues) page for open tasks or bugs that need fixing.

## Roadmap

The following features and improvements are planned for future releases:
(Note: "-" means WIP and partially implemented but unstable, while "/" means WIP but not public. ✔ of course means implemented.)
- [-] Environment variables management system
- [-] Enhanced command auto-completion
- [ ] Robust configuration system
- [ ] Quoting and escaping mechanisms
- [ ] Alias command support
- [ ] Scripting capabilities (if, elif, else, for, while, functions, variables)
- [ ] Wildcards and regex support
- [ ] Enhanced command-line options
- [ ] Improved argument handling for built-in commands
- [✔] Support for popular, complex commands and text editors (e.g., Nano, Vim)

## Disclaimer

Nash is a work in progress and may not be suitable for production use. Use at your own risk. The project is developed by an amateur, self-taught developer primarily experienced in C#. As such, the code quality may vary, and improvements are constantly being made based on feedback and contributions from the community.

## License

[MIT License](LICENSE)

---

For more information, bug reports, or feature requests, please visit the [Nash GitHub repository](https://github.com/barely-a-dev/Nash).