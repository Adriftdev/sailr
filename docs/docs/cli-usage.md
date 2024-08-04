---
sidebar_position: 2
---

# CLI Usage

### Basic Commands

| Command        | Description                                           | Example                             |
| -------------- | ----------------------------------------------------- | ----------------------------------- |
| `sailr init`   | Initialize a new environment.                        | `sailr init my-environment`        |
| `sailr deploy` | Deploy an environment to your Kubernetes cluster.     | `sailr deploy my-environment`     |
| `sailr generate` | Generate manifests without deploying.                | `sailr generate my-environment`   |
| `sailr build`  | Build container images for your services.           | `sailr build my-environment`      |
| `sailr go`     | Combine build, generate, and deploy in one step.     | `sailr go my-environment`         |
| `sailr completions` | Generate shell completion scripts (bash or zsh). | `sailr completions bash`         |
| `sailr --help` | Show help information for all Sailr commands.          | `sailr --help`                      | 

### Additional Options

* `--force` (with `build`): Force rebuild all service images, ignoring the cache.
