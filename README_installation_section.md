## Installation

Before you begin, ensure you have the following prerequisites installed on your system:

*   **Docker:** Essential for building your service container images.
*   **OpenTofu (Recommended) or Terraform:** Used by Sailr for infrastructure management tasks, particularly for local development clusters.

The quickest way to install Sailr on Linux and macOS is by using our `install.sh` script. This script handles downloading the correct Sailr binary for your system, installing it (defaulting to `$HOME/bin`), and can also assist with setting up shell completions. It may also offer to install missing dependencies like Docker or OpenTofu.

```bash
curl -sfL https://raw.githubusercontent.com/YOUR_ORG/sailr/main/install.sh | sh -s -- -b $HOME/bin
```
*(Please replace `YOUR_ORG/sailr` with the actual GitHub repository path for Sailr.)*

This command downloads and executes the installation script. The `-b $HOME/bin` argument specifies the installation directory for the `sailr` binary.

For more comprehensive instructions, including manual installation steps for various platforms, detailed dependency setup, checksum verification, and advanced shell completion configuration, please refer to our **[Full Installation Guide](docs/docs/getting-started/installation.md)** in the official documentation.

**Part 2: Docusaurus Installation Page (`docs/docs/getting-started/installation.md`)**

This is the content for the new Docusaurus page.Okay, I've created the content for the README.md installation section.

Now, I will create the Docusaurus installation page.
