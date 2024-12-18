#!/bin/bash

set -e

echo "Starting Nash installer..."

# Check if Rust is installed
if ! command -v rustc &> /dev/null
then
    echo "Rust is not installed. Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source $HOME/.cargo/env
    echo "Rust installed successfully."
else
    echo "Rust is already installed."
fi

# Navigate to the directory containing the script
cd "$(dirname "$0")"

# Build the Nash project in release mode
echo "Building Nash in release mode..."
cargo build --release

# Check if the Nash binary was successfully created
if [ ! -f "./target/release/nash" ]; then
    echo "Error: Nash binary was not created. Build process may have failed."
    exit 1
fi

# Copy the Nash binary to /usr/bin with error checking
echo "Copying Nash binary to /usr/bin..."
if sudo cp ./target/release/nash /usr/bin/nash; then
    echo "Nash binary successfully copied to /usr/bin."
else
    echo "Error: Failed to copy Nash binary to /usr/bin. Please check your permissions and try again."
    exit 1
fi

# Set appropriate permissions for Nash
sudo chmod 755 /usr/bin/nash

# Build the nash_build_mgr project in release mode
echo "Building nash_build_mgr in release mode..."
cargo build --release --manifest-path=nash_build_mgr/Cargo.toml

# Check if the nash_build_mgr binary was successfully created
if [ ! -f "./nash_build_mgr/target/release/nash_build_mgr" ]; then
    echo "Error: nash_build_mgr binary was not created. Build process may have failed."
    exit 1
fi

# Copy the nash_build_mgr binary to /usr/bin with error checking
echo "Copying nash_build_mgr binary to /usr/bin..."
if sudo cp ./nash_build_mgr/target/release/nash_build_mgr /usr/bin/nbm; then
    echo "nash_build_mgr binary successfully copied to /usr/bin."
else
    echo "Error: Failed to copy nash_build_mgr binary to /usr/bin. Please check your permissions and try again."
    exit 1
fi

# Set appropriate permissions for nash_build_mgr
sudo chmod 755 /usr/bin/nbm

echo "Nash and nash_build_mgr have been successfully installed!"
echo "You can now use 'nash' and 'nbm' commands from anywhere in your terminal."

# Version check with error handling for Nash
if command -v nash &> /dev/null; then
    echo "Installed Nash version:"
    if nash --version; then
        echo "Nash installation completed successfully."
    else
        echo "Warning: Nash was installed but failed to run. Please check for any error messages above."
    fi
else
    echo "Warning: Nash installation might have failed. The 'nash' command is not recognized."
    echo "Please check the output above for any errors and try running the installer again."
fi
