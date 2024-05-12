#!/bin/sh

# Script version
VERSION="1.2.0"


# Temporary directory for downloads (use user's home directory for safety)
DOWNLOAD_DIR=$(mktemp -d --tmpdir=$HOME)


if ! command -v docker &> /dev/null; then
  echo "Docker is not installed. Please ensure docker is installed on your system."
  if [[ $EUID -ne 0 ]]; then
    echo "Would you like to install Docker now? (y/N) "
    read -r install_docker
    if [[ $install_docker == "y" ]]; then
      # Add Docker's official GPG key:
      sudo apt-get update
      sudo apt-get -y install ca-certificates curl
      sudo install -m 0755 -d /etc/apt/keyrings
      sudo curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
      sudo chmod a+r /etc/apt/keyrings/docker.asc

      # Add the repository to Apt sources:
      echo \
        "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu \
        $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | \
        sudo tee /etc/apt/sources.list.d/docker.list > /dev/null
      sudo apt-get update
      sudo apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
      sudo groupadd docker
      sudo usermod -aG docker $USER
      newgrp docker
    else
      exit 1
    fi
  fi
fi

if ! command -v tofu &> /dev/null; then
  echo "Tofu is not installed. Please ensure tofu is installed on your system."
  if [[ $EUID -ne 0 ]]; then
    echo "Would you like to install tofu now? (y/N) "
    read -r install_tofu
    if [[ $install_tofu == "y" ]]; then
      # Download the installer script:
      curl --proto '=https' --tlsv1.2 -fsSL https://get.opentofu.org/install-opentofu.sh -o install-opentofu.sh
      # Alternatively: wget --secure-protocol=TLSv1_2 --https-only https://get.opentofu.org/install-opentofu.sh -O install-opentofu.sh

      # Grant execution permissions:
      chmod +x install-opentofu.sh

      # Please inspect the downloaded script at this point.

      # Run the installer:
      ./install-opentofu.sh --install-method standalone

      # Remove the installer:
      rm install-opentofu.sh
    else
      exit 1
    fi
  fi
fi

if ! command -v minikube &> /dev/null; then
  echo "Minikube is not installed. Please ensure minikube is installed on your system, to support the optional default develop cluster."
  if [[ $EUID -ne 0 ]]; then
    echo "Would you like to install Minikube now? (y/N) "
    read -r install_minikube
    if [[ $install_minikube == "y" ]]; then

      curl -LO https://storage.googleapis.com/minikube/releases/latest/minikube-linux-amd64
      sudo install minikube-linux-amd64 /usr/local/bin/minikube
    else
      exit 1
    fi
  fi
fi

# CLI name (replace with your actual CLI name)
CLI_NAME="sailr"

# Download URL for the pre-built CLI binary (replace with your appropriate URL)
CLI_DIR="https://github.com/Adriftdev/sailr/releases/download/v$VERSION"
CLI_URL="$CLI_DIR/sailr-v$VERSION-unknown-linux-gnu"


# Check for macOS-specific download (if applicable)
if [ $(uname) = "Darwin" ]; then
  CLI_URL="$CLI_DIR/sailr-v$VERSION-apple-darwin-arm64"
fi

# Download the CLI binary
echo "Downloading $CLI_NAME..."
curl -fsSL "$CLI_URL" -o "$DOWNLOAD_DIR/$CLI_NAME"

# Check for download success
if [ $? -ne 0 ]; then
  echo "Error downloading $CLI_NAME. Please check the download URL."
  exit 1
fi

# Set executable permissions (adjust if needed)
chmod +x "$DOWNLOAD_DIR/$CLI_NAME"

# Installation directory (modify as desired, use user's bin directory)
INSTALL_DIR="$HOME/bin"  # User-specific bin directory

# Check if the user's bin directory exists
if [ ! -d "$INSTALL_DIR" ]; then
  echo "Creating user bin directory: $INSTALL_DIR"
  mkdir -p "$INSTALL_DIR"
fi

# Check write permissions for the user's bin directory
if [ ! -w "$INSTALL_DIR" ]; then
  echo "The installation directory '$INSTALL_DIR' requires write permissions. Please adjust file permissions manually."
  exit 1
fi

# Move the binary to the user's bin directory
echo "Installing $CLI_NAME..."
mv "$DOWNLOAD_DIR/$CLI_NAME" "$INSTALL_DIR/$CLI_NAME"

# Cleanup temporary directory
rm -rf "$DOWNLOAD_DIR"

echo "Installation complete! Add '$INSTALL_DIR' to your PATH environment variable to use sailr from any directory."

# Add sailr completions for the default shell (optional, user-scoped)
SHELL=${SHELL##*/}  # Get only the basename of the current shell
case "$SHELL" in
  bash|zsh)
    SAILR_COMPLETIONS_DIR="$HOME/.local/share/$SHELL/completions"
    ;;
  elvish)
    SAILR_COMPLETIONS_DIR="$HOME/.config/elvish/completions"
    ;;
  fish)
    SAILR_COMPLETIONS_DIR="$HOME/.config/fish/completions"
    ;;
  powershell)
    echo "PowerShell completions not currently supported."
    ;;
  *)
    echo "Unsupported shell: $SHELL. Sailr completions not installed."
    ;;
esac

if [ -n "$SAILR_COMPLETIONS_DIR" ]; then
  echo "Installing sailr completions for $SHELL..."
  mkdir -p $SAILR_COMPLETIONS_DIR
  # Download to user-writable directory
  curl -fsSL https://raw.githubusercontent.com/Adriftdev/sailr/main/completion/sailr.$SHELL > "$SAILR_COMPLETIONS_DIR/sailr"
  echo "please run: source $SAILR_COMPLETIONS_DIR/sailr"
fi
