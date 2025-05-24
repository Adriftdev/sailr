---
sidebar_position: 1
title: Installation
---

# Sailr Installation Guide

This guide provides comprehensive instructions for installing Sailr and its dependencies.

## Prerequisites

Before installing Sailr, you need the following tools installed on your system:

*   **Docker:** Sailr uses Docker to build container images for your services.
    *   [Install Docker Engine](https://docs.docker.com/engine/install/) (Choose your OS)
*   **OpenTofu (Recommended) or Terraform:** Sailr uses OpenTofu/Terraform for managing infrastructure components, particularly for local development environments with Minikube. Sailr aims to support both.
    *   [Install OpenTofu](https://opentofu.org/docs/intro/install/)
    *   [Install Terraform](https://developer.hashicorp.com/terraform/tutorials/aws-get-started/install-cli)

## Installation Methods

### Method 1: Using the `install.sh` Script (Recommended for Linux & macOS)

This is the easiest way to install Sailr. The script will:
1.  Detect your OS and architecture.
2.  Download the latest Sailr binary from GitHub Releases.
3.  Install it to a directory in your user's PATH (default: `$HOME/bin`).
4.  Optionally offer to install Docker, OpenTofu, and Minikube if they are not found (may require `sudo`).
5.  Install shell completion scripts for Bash, Zsh, Fish, and Elvish.

To install using this method:

```bash
curl -sfL https://raw.githubusercontent.com/YOUR_ORG/sailr/main/install.sh | sh -s -- -b $HOME/bin
```
*(Remember to replace `YOUR_ORG/sailr` with the actual repository path if different)*

**Note:**
*   The script will inform you where `sailr` is installed. Ensure this location is in your `$PATH`. If `$HOME/bin` is not in your path, add `export PATH="$HOME/bin:$PATH"` to your shell configuration file (e.g., `~/.bashrc`, `~/.zshrc`).
*   The `install.sh` script itself can be inspected before running if you have security concerns:
    ```bash
    curl -sfL https://raw.githubusercontent.com/YOUR_ORG/sailr/main/install.sh -o install-sailr.sh
    # Review install-sailr.sh
    sh ./install-sailr.sh -b $HOME/bin
    # rm ./install-sailr.sh # Optional: remove after use
    ```

### Method 2: Manual Installation from GitHub Releases

If you prefer not to use the script or are on a different OS (e.g., Windows, though official Windows binaries may not yet be available), you can install Sailr manually:

1.  **Go to the [Sailr GitHub Releases page](https://github.com/YOUR_ORG/sailr/releases).** (Replace `YOUR_ORG/sailr` with the actual repository path)
2.  **Download the appropriate binary** for your operating system and architecture (e.g., `sailr-vX.Y.Z-x86_64-unknown-linux-musl.tar.gz` or `sailr-vX.Y.Z-aarch64-apple-darwin.tar.gz`).
3.  **(Security Recommended)** Download the checksum file (e.g., `sha256sums.txt` or `sailr-vX.Y.Z_checksums.txt`) for the release and verify the integrity of your downloaded archive.
    ```bash
    # Example for Linux/macOS (assuming sha256sums.txt is in the current directory):
    sha256sum -c sha256sums.txt --ignore-missing
    ```
    *Ensure the command output shows `OK` for your downloaded file.*
4.  **Extract the archive.** This will typically contain the `sailr` executable and potentially other assets like `LICENSE` and `README`.
    ```bash
    # Example for Linux/macOS:
    tar -xzf sailr-vX.Y.Z-x86_64-unknown-linux-musl.tar.gz 
    # (Adjust filename as per your download)
    ```
5.  **Move the `sailr` executable** to a directory in your system's PATH. Common choices are `/usr/local/bin` (requires sudo) or `$HOME/bin` (user-local).
    ```bash
    # For system-wide installation (requires sudo):
    sudo mv sailr /usr/local/bin/sailr

    # For user-local installation:
    mkdir -p $HOME/bin
    mv sailr $HOME/bin/sailr
    ```
    *If you use `$HOME/bin`, ensure it's in your `$PATH`.*
6.  **Ensure the binary is executable:**
    ```bash
    # If installed to /usr/local/bin (requires sudo):
    sudo chmod +x /usr/local/bin/sailr 
    # If installed to $HOME/bin:
    chmod +x $HOME/bin/sailr
    ```

## Shell Completions

Sailr provides shell completions to enhance your CLI experience. You can generate them using the `sailr completions [shell_name]` command. The `install.sh` script also attempts to set these up.

Supported shells are: Bash, Elvish, Fish, PowerShell, and Zsh.

### For Bash:
Add the following to your `~/.bashrc` file (or `~/.bash_profile` on macOS):
```bash
if command -v sailr &> /dev/null; then
  source <(sailr completions bash)
fi
```
*Restart your shell or source the file (e.g., `source ~/.bashrc`).*

### For Zsh:
Add the following to your `~/.zshrc` file:
```bash
if command -v sailr &> /dev/null; then
  source <(sailr completions zsh)
fi
```
*Restart your shell or source the file (e.g., `source ~/.zshrc`).*

For more advanced Zsh setups (like with Oh My Zsh), you might output the script to a file:
```bash
# Example for Oh My Zsh (ensure the directory exists):
# mkdir -p ${ZSH_CUSTOM:-~/.oh-my-zsh/custom}/plugins/sailr
# sailr completions zsh > ${ZSH_CUSTOM:-~/.oh-my-zsh/custom}/plugins/sailr/_sailr
# Then add 'sailr' to the plugins array in your .zshrc
```

### For Fish:
Run this in your Fish shell:
```fish
mkdir -p ~/.config/fish/completions
sailr completions fish > ~/.config/fish/completions/sailr.fish
```

### For Elvish:
Generate the script:
```elvish
sailr completions elvish > sailr_completions.elv
```
Then, load this script from your `rc.elv` file. Refer to Elvish documentation for details.

### For PowerShell:
Generate the script and add it to your PowerShell profile:
```powershell
sailr completions powershell | Out-String | Invoke-Expression
# To make it permanent, add the above line to your profile script.
# Find your profile path by running: echo $PROFILE
```

## Verifying Installation

After installation, verify that Sailr is working correctly and is accessible in your PATH:
```bash
sailr --version
```
You should see the installed version of Sailr printed, e.g., `sailr vX.Y.Z`.

## Next Steps

With Sailr installed, you're ready to start managing your Kubernetes environments!
*   Head over to our **[Getting Started Tutorial](tutorial.md)** to create and deploy your first application.
*   Familiarize yourself with the **[CLI Usage Guide](../cli-usage.md)** for a detailed overview of all commands and options.
---
This content is for the Docusaurus page `docs/docs/getting-started/installation.md`.
