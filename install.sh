# Script version
VERSION="1.2.0"


# Temporary directory for downloads (use user's home directory for safety)
DOWNLOAD_DIR=$(mktemp -d --tmpdir=$HOME)


# Function to install a package using the detected package manager
# (Modify or add logic for other package managers if needed)
install_package() {
  package_name="$1"
  if [[ "$package_manager" ]]; then
    echo "Installing $package_name using $package_manager..."
    sudo $package_manager install -y "$package_name"
  else
    echo "Package manager not found. Manual installation of $package_name might be required."
  fi
}


# Check for Docker
if ! command -v docker &> /dev/null; then
  echo "Docker is not installed. Please ensure docker is installed on your system."
  if [[ $EUID -ne 0 ]]; then
    echo "Would you like to install Docker now? (y/N) "
    read -r install_docker
    if [[ $install_docker == "y" ]]; then
      # **For Linux:** Use the detected package manager (consider limitations in WSL)
      if [[ "$os_type" == "Linux" ]]; then
        package_manager="$package_manager"  # Already detected earlier
        install_package "docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin"
      # **For macOS:** Use brew if available
      elif [[ "$os_type" == "Darwin" ]]; then
        package_manager="brew"
        if which brew &> /dev/null; then
          sudo brew install docker docker-compose
        else
          echo "brew not found. Refer to Docker documentation for macOS installation: https://docs.docker.com/engine/install/macos/"
        fi
      fi
      # User needs to install Docker Desktop for Windows for seamless experience
      else
        echo "**WSL Limitation:** Installing Docker directly in WSL might have limitations."
        echo "Consider using Docker Desktop for Windows for a more integrated experience: https://docs.docker.com/desktop/install/windows-install/"
      fi
    else
      exit 1
    fi
  fi



# Check for Tofu
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


# Check for Minikube (optional)
if ! command -v minikube &> /dev/null; then
  echo "Minikube is not installed. Please ensure minikube is installed on your system, to support the optional default develop cluster."
  if [[ $EUID -ne 0 ]]; then
    echo "Would you like to install Minikube now? (y/N) "
    read -r install_minikube
    if [[ $install_minikube == "y" ]]; then
      # **For Linux:** Use the detected package manager (consider limitations in WSL)
      if [[ "$os_type" == "Linux" ]]; then
        package_manager="$package_manager"  # Already detected earlier
        # Replace with appropriate installation command for your Linux distribution
        # (this might involve adding repositories first)
        # ...
      # **For macOS

      elif [[ "$os_type" == "Darwin" ]]; then
        echo "Minikube installation on macOS is recommended using Homebrew cask."
        echo "Refer to Minikube documentation for macOS installation: https://docs.minikube.k8s.io/docs/install/mac/"
      fi
      # User likely needs to install Minikube manually on Windows
      else
        echo "**WSL Limitation:** Installing Minikube directly in WSL might have limitations."
        echo "Refer to Minikube documentation for Windows installation: https://docs.minikube.k8s.io/docs/install/windows/"
      fi
    else
      exit 1
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
INSTALL_DIR="$HOME/bin"  # Preferred location (check for write access)
if [ ! -d "$INSTALL_DIR" -o ! -w "$INSTALL_DIR" ]; then
  echo "User bin directory '$INSTALL_DIR' doesn't exist or lacks write permissions."
  echo "Consider using an alternative location with write access (e.g., /usr/local/bin)."
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
