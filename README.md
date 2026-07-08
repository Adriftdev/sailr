## Sailr: A Kubernetes Management CLI for Smooth Sailing
[![Rust CI](https://github.com/Adriftdev/sailr/actions/workflows/rust.yml/badge.svg)](https://github.com/Adriftdev/sailr/actions/workflows/rust.yml)
[![Sailr Workflow - ci](https://github.com/Adriftdev/sailr/actions/workflows/sailr-ci.yml/badge.svg)](https://github.com/Adriftdev/sailr/actions/workflows/sailr-ci.yml)

### Sailr: The Calming Force in the Choppy Waters of Kubernetes

Kubernetes is a powerful tool for managing containerized applications, but it can also be complex and challenging to use. If you're feeling overwhelmed by Kubernetes, Sailr can help. Sailr is an environment management CLI that makes it easy to deploy, manage, and troubleshoot Kubernetes applications. With Sailr, you can:

- Automate deployments and updates so you can sail through your work.
- Manage resources efficiently so you don't run aground.
- Opinioned kubernetes infrastructure automation.

## Roadmap
- Zero downtime deployments.
- Sailr Workflow Stablization and cleanup.
- Remove the dependency on OpenTofu, while keeping support for both OpenTofu/Terraform.
- Helm support
- Full handlebar like templating support - need to investigate best approach for this, that allows flow control and intuitive syntax.

Sailr is the perfect tool for Kubernetes users who want to save time, reduce stress, and get more out of their Kubernetes deployments. Try Sailr today and see the difference it can make.

## Installation

Before you begin, ensure you have the following prerequisites installed:
*   **Docker:** For building service images.
*   **OpenTofu (or Terraform):** For infrastructure management.

The easiest way to install Sailr on Linux and macOS is by using our `install.sh` script. This script will attempt to install `sailr` to `$HOME/bin` and can also help install dependencies.

```bash
curl -sfL https://raw.githubusercontent.com/Adriftdev/sailr/main/install.sh | sh -s -- -b $HOME/bin
```

For more detailed instructions, including manual installation, setting up shell completions, and further details on dependencies, please see our [Full Installation Guide](docs/docs/getting-started/installation.md).

### System Requirements

- OpenTofu (Terraform replacement)
- Docker

## Minikube Setup

```bash
minikube start --driver=docker --download-only
```

## CLI Usage

### Initialization

Initializes a new environment named <environment_name>. Creates a directory structure, a default configuration file, and a "sample-app" service to get you started quickly.

```bash 
sailr init <environment_name> 
```

### Add Service

Adds a new service to your project. This creates boilerplate Kubernetes templates (`deployment.yaml`, `service.yaml`, `configmap.yaml`) in `k8s/templates/<service_name>/` and adds the service to the `develop` environment's configuration.

```bash
sailr add service <service_name> --type <app_type>
```

### Completions

Generates shell completion scripts for bash or zsh to enhance the Sailr CLI experience.

```bash 
sailr completions [bash|zsh] 
```

### Deployment

Deploys an existing environment named <environment_name> to a specified Kubernetes cluster context.

```bash 
sailr deploy <environment_name> 
```

### Generation

Generates deployment manifests for services defined in the <environment_name> environment configuration file without deploying them to the cluster.

```bash 
sailr generate <environment_name> 
```

### Building

Builds container images for services in the <environment_name> environment. Optionally excludes services listed in <service1,service2,...> (comma-separated) from the build process.

Sailr supports two build backends:

- Roomservice: current default backend.
- runkernel: experimental workflow-backed backend.

Use `--engine runkernel` or `[build].engine = "runkernel"` to try the new backend. Roomservice remains available with `--engine roomservice`.

Roomservice stores build cache under `.roomservice`. The runkernel backend stores Sailr-owned build cache under `.sailr/cache/build`, keeping embedded runkernel state inside Sailr's project cache instead of exposing `.runkernel` as a user-facing project directory.

`[build].max_parallelism` is accepted by the runkernel backend, but not enforced yet. Sailr emits a warning when this setting is used with `engine = "runkernel"`.

```bash 
sailr build <environment_name> [--ignore <service1,service2,...>]
sailr build --name dev --engine runkernel
```

```toml
[build]
engine = "runkernel"
fail_fast = false
```

### Combined Workflow

Combines generation and deployment in a single command for the <environment_name> environment.

```bash 
sailr go <environment_name>
```

### Additional Notes

- Use the --force flag with build to rebuild all service images regardless of the cache.
- For a full list of commands and their detailed options, please see the [CLI Command Reference](docs/docs/cli-usage.md).

### Getting Help

- Consult the main Sailr project documentation [here](docs/docs/intro.md).
- For detailed CLI command information, refer to the [CLI Command Reference](docs/docs/cli-usage.md).

## Sailr Configuration File

This document outlines the configuration options for the Sailr CLI application, used for generating and deploying services to a Kubernetes cluster.

### Schema Version

* **schema_version (string):** (Required) The schema version of the configuration file. New projects should use `0.5.0`.

### Global Configuration

These settings apply globally and can be referenced within templates using double curly braces (`{{ }}`).

* **name (string):** (Required) The name of the environment. Used for identification and template replacement (e.g., `{{env_name}}`).
* **log_level (string):** (Optional) The desired logging level for the Sailr CLI application itself. Defaults to "INFO".
* **domain (string):** (Required) The domain name for your services. Used throughout configurations and templates.
* **default_replicas (integer):** (Optional) The default number of replicas for deployed services. Defaults to 1.
* **registry (string):** (Optional) The container image registry to use for deployments. Defaults to "docker.io".

### Services

Services are defined with `[[service]]` entries. Each service can also include build configuration shared by both build backends.

Sailr supports pluggable build backends.

Roomservice is the current default backend. The experimental runkernel backend can be selected with `--engine runkernel` or `[build].engine = "runkernel"`.

Build configuration lives in `config.toml` and is shared by both backends.

```toml
[[service]]
name = "api"
version = "1.2.3"

[service.build]
path = "services/api"
include = ["src/**/*.rs", "Cargo.toml", "Dockerfile"]
build_command = "docker buildx build -t {{ registry }}/{{ name }}:{{ version }} ."
push_command = "docker push {{ registry }}/{{ name }}:{{ version }}"
```

Older configs may still use `service_whitelist`; migrate to schema `0.5.0` and `[[service]]` for new projects. See the [config.toml Guide](docs/docs/configuration/config-toml.md) and [Roomservice to runkernel migration guide](docs/docs/migration/roomservice-to-runkernel.md) for details.

The Roomservice backend is based on roomservice-rust. Credit to [Curtis Wilkinson](https://github.com/curtiswilkinson/roomservice-rust) for the original Roomservice implementation.

### Environment Variables

This section defines environment variables used during service generation and injected into templates.

* **[[environment_variables]] (array):** An array of environment variable definitions. Each environment variable definition has the following properties:
    * **name (string):** (Required) The name of the environment variable.
    * **value (string):** (Required) The value to be assigned to the environment variable. This value will be replaced with `{{name}}` in templates.

## Core Functionalities

Sailr provides a rich set of commands for interacting with service deployments:

* **Initialization (init):** Initializes a new environment by copying base templates and creating a default configuration file.
* **Completions (completions):** Generates shell completion scripts for popular shells to enhance the CLI experience.
* **Deployment (deploy):** Deploys an existing environment to a specified Kubernetes cluster context.
* **Generation (generate):** Generates deployment manifests for an environment without deploying them to the cluster.
* **Building (build):** Builds container images for services in an environment. Skips services already built unless the `--force` flag is used.
* **Combined Workflow (go):** Combines build, generation, and deployment in a single command, streamlining the process.

## Advanced Usage and Gotchas

### Generating Specific Services

By default, the `generate` command processes all services defined in the environment configuration. You can use the `--ignore` flag with the `Build` and `Go` 
commands to specify services to exclude from the build process. The flag accepts comma-separated service names.

```bash
sailr build my-env --ignore service1,service3
```

This command will build all services in the my-env environment except service1 and service3.
Building without Deployment

The Build command focuses solely on building service images. It doesn't deploy them to a Kubernetes cluster. Use the Go command for combined generation and deployment.
Service Build Caching

Sailr utilizes a basic build cache to prevent unnecessary rebuilds. The Build command respects the cache by default. 
Use the `--force` flag to force all services to rebuild, regardless of the cache state.


```bash
sailr build my-env --force
```

Use code with caution.

This command will rebuild all services in the my-env environment, even if they exist in the cache.
Environment Providers

Currently, Sailr supports deploying to a local Kubernetes cluster using the LocalK8 infrastructure provider. 
Future versions might introduce support for additional cloud providers like GCP and AWS.

## Contributing

Sailr an open-source project. If you're interested in contributing to Sailr's development, look for a contributing guide in the project's repository. 
It will outline the process for submitting bug reports, feature requests, and code patches.
