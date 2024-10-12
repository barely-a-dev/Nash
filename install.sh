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

# Build the project in release mode
echo "Building Nash in release mode..."
cargo build --release

# Copy the binary to /usr/bin
echo "Copying Nash binary to /usr/bin..."
sudo cp ./target/release/nash /usr/bin/nash

echo "Nash has been successfully installed!"
echo "You can now use 'nash' command from anywhere in your terminal."

# Optionally, you can add a version check
if command -v nash &> /dev/null
then
    echo "Installed Nash version:"
    nash --version
else
    echo "Warning: Nash installation might have failed. Please check the output above for any errors."
fi