## Installation

Before you begin, ensure you have the following prerequisites installed:
*   **Docker:** For building service images.
*   **OpenTofu (or Terraform):** For infrastructure management (Sailr aims to support both).

The easiest way to install Sailr on Linux and macOS is by using our `install.sh` script. This script will download the latest Sailr binary, install it to `$HOME/bin` (by default), and can help you install dependencies.

```bash
curl -sfL https://raw.githubusercontent.com/YOUR_ORG/sailr/main/install.sh | sh -s -- -b $HOME/bin
```

For more detailed instructions, including manual installation steps, platform-specific guidance, and dependency setup, please see our [Full Installation Guide](docs/getting-started/installation.md).

**Part 2: Docusaurus Installation Page (`docs/docs/getting-started/installation.md`)**
