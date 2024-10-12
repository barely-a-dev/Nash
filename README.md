# Installation
- Prerequirements: Rust, Git
## Manual Install
- Clone the repo (git clone https://github.com/barely-a-dev/Nash.git)
- Run the install.sh script (cd Nash && chmod +x ./install.sh && ./install.sh)
## Automated Install
- Download a release from https://github.com/barely-a-dev/Nash/releases
- Run it with force update flags (cd /home/<username>/Downloads && chmod +x nash && nash --update -f)
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

## Development Status

Nash is currently in early development (v0.0.7). Many crucial features are planned or in progress, including:

- Environment variables
- Command auto-completion improvements
- Configuration system
- Quoting and escaping
- Alias command
- Scripting capabilities (if, elif, else, for, while, functions, variables)
- Wildcards and [regex](https://en.wikipedia.org/wiki/Regular_expression) support
- Enhanced command-line options
- Improved argument handling for built-in commands
- [Nano](https://www.nano-editor.org/) support

## Contributing

Contributions are welcome! Please feel free to submit a [Pull Request](https://docs.github.com/en/pull-requests).
If you don't know how to contribute, open an [issue](https://github.com/barely-a-dev/Nash/issues/new) with the enhancement label.

## Disclaimer

Nash is a work in progress and may not be suitable for production use. Use at your own risk. Additionally: I am an amateur, self-taught developer who primarily uses C#; a language this project does not use. In any case, do not expect top-quality code until my mistakes are pointed out to me, or a pull request fixing them is approved.
