---
sidebar_position: 2
---

# CLI Usage

### Basic Commands

| Command        | Description                                           | Example                             |
| -------------- | ----------------------------------------------------- | ----------------------------------- |
| `sailr init`   | Initialize a new environment.                        | `sailr init -n my-environment`        |
| `sailr deploy` | Deploy an environment to your Kubernetes cluster.     | `sailr deploy -n my-environment -c my-context-name`     |
| `sailr generate` | Generate manifests without deploying.                | `sailr generate -n my-environment`   |
| `sailr build`  | Build container images for your services.           | `sailr build my-environment`      |
| `sailr go`     | Combine build, generate, and deploy in one step.     | `sailr go -n my-environment -c my-context-name`         |
| `sailr completions` | Generate shell completion scripts (bash or zsh). | `sailr completions bash`         |
| `sailr --help` | Show help information for all Sailr commands.          | `sailr --help`                  | 

### Additional Options

* `--force` (with `build`): Force rebuild all service images, ignoring the cache.
* **Generating Specific Services:** Use the `--ignore` flag with the `build` and `go` commands. 
* **Building without Deployment:** The `build` command focuses only on image creation.
* **Service Build Caching:** Use `--force` to rebuild images even if they are cached.
