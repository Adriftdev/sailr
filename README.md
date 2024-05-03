## Sailr: A Kubernetes Management CLI for Smooth Sailing

### Sailr: The Calming Force in the Choppy Waters of Kubernetes

Kubernetes is a powerful tool for managing containerized applications, but it can also be complex and challenging to use. If you're feeling overwhelmed by Kubernetes, Sailr can help. Sailr is an environment management CLI that makes it easy to deploy, manage, and troubleshoot Kubernetes applications. With Sailr, you can:

- Automate deployments and updates so you can sail through your work.
- Manage resources efficiently so you don't run aground.
- Troubleshoot problems quickly and easily so you can stay afloat.

Sailr is the perfect tool for Kubernetes users who want to save time, reduce stress, and get more out of their Kubernetes deployments. Try Sailr today and see the difference it can make.

### System Requirements

- OpenTofu (Terraform replacement)
- Docker

## CLI Usage

### Initialization

Initializes a new environment named <environment_name>. Creates a directory structure and a default configuration file.

```bash 
sailr init <environment_name> 
```

### Completions

Generates shell completion scripts for bash or zsh to enhance the Sailr CLI experience.

```bash 
sailr completions [bash|zsh] 
```

### Environment Management 

**(depricated - This happens in init, use config templates and k8s/default.toml to change default config)**

Creates a new environment named <environment_name> with optional local service pods like PostgreSQL and Redis (intended for development environments).

```bash 
sailr env create <environment_name> 
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

```bash 
sailr build <environment_name> [--ignore <service1,service2,...>]
```

### Combined Workflow

Combines generation and deployment in a single command for the <environment_name> environment.

```bash 
sailr go <environment_name>: 
```

### Additional Notes

- Use the --force flag with build to rebuild all service images regardless of the cache.
- Refer to the documentation for detailed configuration options and advanced usage.

### Getting Help

- Consult the Sailr project documentation [here]().

## Sailr Configuration File

This document outlines the configuration options for the Sailr CLI application, used for generating and deploying services to a Kubernetes cluster.

### Schema Version

* **schema_version (string):** (Required) The schema version of the configuration file. Currently set to `0.2.0`. Changing this version might indicate breaking changes, new features, or patches to the Sailr config specification.

### Global Configuration

These settings apply globally and can be referenced within templates using double curly braces (`{{ }}`).

* **name (string):** (Required) The name of the environment. Used for identification and template replacement (e.g., `{{env_name}}`).
* **log_level (string):** (Optional) The desired logging level for the Sailr CLI application itself. Defaults to "INFO".
* **domain (string):** (Required) The domain name for your services. Used throughout configurations and templates.
* **default_replicas (integer):** (Optional) The default number of replicas for deployed services. Defaults to 1.
* **registry (string):** (Optional) The container image registry to use for deployments. Defaults to "docker.io".

### Service Whitelist

This section defines the services to be generated and deployed.

* **[[service_whitelist]] (array):** An array of service definitions. Each service definition within the whitelist has the following properties:
    * **name (string):** (Required) The name of the service. Used for image pulling and as a reference in templates (`{{service_name}}`).
    * **version (string):** (Required) The version of the service image (semver or tag). Used in templates (`{{service_version}}`).
    * **path (string):** (Optional) The path to the service template directory relative to `k8s/templates`. Defaults to the service name.
    * **namespace (string):** (Optional) The namespace where the service will be deployed in Kubernetes. Defaults to the environment name.
    * **build (boolean):** (Optional) Whether to build the service image before deployment. Defaults to `true`.

### Environment Variables

This section defines environment variables used during service generation and injected into templates.

* **[[environment_variables]] (array):** An array of environment variable definitions. Each environment variable definition has the following properties:
    * **name (string):** (Required) The name of the environment variable.
    * **value (string):** (Required) The value to be assigned to the environment variable. This value will be replaced with `{{name}}` in templates.

### Build Configuration

This section defines build configurations for services. 

Under the hood uses the core of roomservice-rust credit goes to [Curtis Wilkinson](https://github.com/curtiswilkinson/roomservice-rust) for the roomservice code :D.

Some changes to roomservice config have been made for this applciation - config file has been merged into the config.toml and defined them is as below.

* **[build]** (Optional) Global roomservice configs. Each build configuration can have the following properties:
    * **beforeAll (string):** (Optional) A shell command to execute before all service builds.
    * **[build.rooms.] (object):** Build configuration for a specific service directory. The key `` corresponds to the service name or a custom name.
        * **path (string):** (Required) The path to the service build directory relative to the project root.
        * **run_parallel (string):** (Optional) A shell command to run in parallel for all builds.
        * **before (string):** (Optional) A shell command to run before building the service image.
        * **after (string):** (Optional) A shell command to run after building the service image.

## Core Functionalities

Sailr provides a rich set of commands for interacting with service deployments:

* **Initialization (init):** Initializes a new environment by copying base templates and creating a default configuration file.
* **Completions (completions):** Generates shell completion scripts for popular shells to enhance the CLI experience.
* **Environment Management (env):** 
    * **Create (create):** Creates a new environment with optional local service pods like PostgreSQL and Redis (intended for development environments).
* **Deployment (deploy):** Deploys an existing environment to a specified Kubernetes cluster context.
* **Generation (generate):** Generates deployment manifests for an environment without deploying them to the cluster.
* **Building (build):** Builds container images for services in an environment. Skips services already built unless the `--force` flag is used.
* **Combined Workflow (go):** Combines generation and deployment in a single command, streamlining the process.

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
Future versions might introduce support for additional cloud providers like GCP (as hinted by the Provider enum).

## Contributing

Sailr an open-source project. If you're interested in contributing to Sailr's development, look for a contributing guide in the project's repository. 
It will outline the process for submitting bug reports, feature requests, and code patches.
