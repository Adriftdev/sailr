#!/bin/bash

# Script version
VERSION="1.2.0"

# Rust installation check and guidance
if ! command -v rustc &> /dev/null; then
  echo "Rust is not currently installed."
  echo "Installing Rust is recommended for a complete development environment."
  echo "For installation instructions, visit the official Rust website: https://www.rust-lang.org/tools/install"
  echo "Would you like to continue with the CLI installation (without Rust compilation capabilities)?"
  select yn in "Yes" "No"; do
    case $yn in
      Yes ) break;;
      No ) exit 0;;
    esac
  done
fi

# Temporary directory for downloads
DOWNLOAD_DIR=$(mktemp -d)

# CLI name (replace with your actual CLI name)
CLI_NAME="sailr"

# Download URL for the pre-built CLI binary (replace with your appropriate URL)
CLI_URL="https://github.com/Adriftdev/sailr/releases/download/v$VERSION/sailr-v$VERSION-unknown-linux-gnu"


# Check for macOS-specific download (if applicable)
if [[ $(uname) == "Darwin" ]]; then
  CLI_URL="https://github.com/Adriftdev/sailr/releases/download/v$VERSION/sailr-v$VERSION-apple-darwin-arm64"
fi

# Download the CLI binary
echo "Downloading $CLI_NAME..."
curl -fsSL "$CLI_URL" -o "$DOWNLOAD_DIR/$CLI_NAME"

# Check for download success
if [[ $? -ne 0 ]]; then
  echo "Error downloading $CLI_NAME. Please check the download URL."
  exit 1
fi

# Set executable permissions (adjust if needed)
chmod +x "$DOWNLOAD_DIR/$CLI_NAME"

# Installation directory (modify as desired)
INSTALL_DIR="/usr/local/bin"

# Check if installation directory requires elevated privileges
if [[ ! -w "$INSTALL_DIR" ]]; then
  echo "The installation directory '$INSTALL_DIR' requires root privileges. Please run the script with sudo:"
  echo "sudo ./install.sh"
  exit 1
fi

# Move the binary to the installation directory
echo "Installing $CLI_NAME..."
mv "$DOWNLOAD_DIR/$CLI_NAME" "$INSTALL_DIR/$CLI_NAME"

# Cleanup temporary directory
rm -rf "$DOWNLOAD_DIR"

echo "Installation complete! Run '$CLI_NAME --help' for more information."
