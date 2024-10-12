
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

Nash is currently in early development (v0.0.5). Many features are planned or in progress, including:

- Environment variables
- Command auto-completion improvements
- Configuration system
- Quoting and escaping
- Alias command
- Scripting capabilities (if, elif, else, for, while, functions, variables)
- Wildcards and regex support
- Enhanced command-line options
- Improved argument handling for built-in commands

## Contributing

Contributions are welcome! Please feel free to submit a [Pull Request](https://docs.github.com/en/pull-requests).

## Disclaimer

Nash is a work in progress and may not be suitable for production use. Use at your own risk. Additionally: I am an amateur, self-taught developer who primarily uses C#; a language this project does not use. In any case, do not expect top-quality code until my mistakes are pointed out to me, or a pull request fixing them is approved.
