---
sidebar_position: 1
title: config.toml Guide
---

# Comprehensive `config.toml` Guide

The `config.toml` file is the heart of your Sailr environment's configuration. It defines global settings, the services to be deployed, build processes, and environment variables. This guide provides a detailed explanation of all available fields.

## File Location

When you initialize a new environment using `sailr init <environment_name>`, a `config.toml` file is created within that environment's directory, typically at `k8s/environments/<environment_name>/config.toml` relative to your project root.

## Top-Level Settings

These settings define the overall behavior and metadata for your environment.

### `schema_version` (string)
*   **Required**
*   Specifies the version of the configuration file schema Sailr should expect.
*   Example: `schema_version = "0.5.0"`
*   Changing this version might indicate breaking changes or new features in the Sailr config specification. Consult the Sailr release notes if you need to change this.

### `extends` (string)
*   **Optional**
*   Names another environment under `k8s/environments/<name>/config.toml` to use as this environment's base.
*   Layered environments must resolve to `schema_version = "0.5.0"`.
*   Example: `extends = "develop"`

### `name` (string)
*   **Required unless `extends` is set**
*   The name of the environment. This is used for identification purposes and can be used as a variable in your templates (e.g., `{{name}}` or `{{env_name}}` - Sailr provides it as `{{name}}` as per `Environment::get_variables`).
*   If an environment extends another environment and omits `name`, Sailr uses the child environment directory name.
*   Example: `name = "production"`

### `log_level` (string)
*   **Optional**
*   The logging level for the Sailr CLI application itself during its operations for this environment.
*   Valid values (case-insensitive): `TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR`, `FATAL`.
*   Defaults to `"INFO"`.
*   Example: `log_level = "DEBUG"`

### `domain` (string)
*   **Required**
*   The primary domain name associated with your services in this environment. This is often used in templates to construct ingress hostnames or other URLs.
*   Example: `domain = "myapp.example.com"` (for production) or `domain = "dev.local"` (for development).

### `default_replicas` (integer)
*   **Optional**
*   The default number of replicas for deployed services if not specified in the service's individual Kubernetes deployment manifest templates. This value is available in templates as `{{default_replicas}}`.
*   Defaults to `1`.
*   Example: `default_replicas = 3`

### `registry` (string)
*   **Optional**
*   The container image registry to use for pulling images if the image name does not include a registry hostname. Also used as a target for images built by Sailr. This value is available in templates as `{{registry}}`.
*   Defaults to `"docker.io"`.
*   Example: `registry = "gcr.io/my-project"` or `registry = "quay.io/my-org"`

## Layered Environments

Layered environments let you keep a complete base environment and define small overrides for derived environments. When Sailr loads the child environment, it resolves the base first and then applies the child values in memory.

```toml
# k8s/environments/production-eu/config.toml
schema_version = "0.5.0"
extends = "production"
domain = "eu.example.com"

[[environment_variables]]
name = "REGION"
value = "eu"

[[service]]
name = "api"
version = "2.1.0"
```

Merge behavior:

*   Top-level scalar values override the base.
*   Tables merge field by field.
*   `[[service]]` entries merge by `name`; child fields override matching base fields, and new services are appended.
*   `[[environment_variables]]` entries merge by `name`; child values override matching base values, and new variables are appended.
*   Other arrays replace the base array.
*   Inheritance can be chained. Cycles are rejected.
*   `sailr add-service` and `sailr bump` write local child overrides instead of flattening the resolved environment.

## Build Policy (`[build]`)

The optional top-level `[build]` table controls global build behavior.

```toml
[build]
engine = "runkernel"
fail_fast = false
max_parallelism = 4
before_all = "echo preparing build"
after_all = "echo finished build"
```

### `engine` (string)
*   **Optional**
*   Selects the build backend for `sailr build` and the build step of `sailr go`.
*   Valid values: `roomservice`, `runkernel`.
*   Default: `roomservice`.
*   The CLI flag wins over config. Selection order is:
    1. CLI `--engine`
    2. `[build].engine`
    3. default Roomservice
*   Example: `engine = "runkernel"`

### `fail_fast` (boolean)
*   **Optional**
*   When enabled, the build backend stops scheduling remaining work after a build failure.

### `max_parallelism` (integer)
*   **Optional**
*   Accepted by Sailr build policy.
*   Roomservice uses this where supported.
*   The runkernel backend currently accepts this setting but does not enforce it yet; Sailr emits a warning when `max_parallelism` is set with `engine = "runkernel"`.

### `before_all` and `after_all` (string or array of strings)
*   **Optional**
*   Commands that run before all selected dirty service builds and after all selected dirty service builds complete successfully.

## Services (`[[service]]`)

This is an array of tables, where each table defines a service to be managed by Sailr.

Older configs may still use `[[service_whitelist]]`; prefer `schema_version = "0.5.0"` and `[[service]]` for new projects.

Each service entry can have the following properties:

### `name` (string)
*   **Required**
*   The name of the service. This is used for identifying the service, naming Kubernetes resources, and as a reference in templates (e.g., `{{service_name}}`). It's also often used as the default name for the Docker image if built by Sailr.
*   **Note:** In some contexts like `Service` deserialization, a combined `namespace/name` format might be seen internally, but for user definition, it's just the service name.
*   Example: `name = "frontend-app"`

### `version` (string)
*   **Required**
*   The version of the service image (e.g., semantic version like `"1.2.3"`, a Docker tag like `"latest"`, or a git commit SHA). This is used in templates (e.g., `{{service_version}}`) to specify the image tag for deployment.
*   Example: `version = "0.5.1"`

### `path` (string)
*   **Optional**
*   The path to the service's Kubernetes manifest template directory, relative to the `k8s/templates/` directory in your Sailr project.
*   If omitted, Sailr defaults this to the service `name` (i.e., Sailr will look for templates in `k8s/templates/<service_name>/`).
*   Example: `path = "custom-frontend-templates"` (would look for templates in `k8s/templates/custom-frontend-templates/`)

### `namespace` (string)
*   **Optional**
*   The Kubernetes namespace where this service will be deployed. This value is available in templates as `{{service_namespace}}`.
*   If omitted, Sailr defaults this to the environment `name` (from the global settings).
*   Example: `namespace = "web-services"`

### Build Configuration (within a `[[service]]` entry)

Sailr integrates a build system to build your service's container images. Roomservice is the current default backend, and the experimental runkernel backend can be selected with `--engine runkernel` or `[build].engine = "runkernel"`. These fields control the build process for a specific service.

#### `build` (string)
*   **Optional**
*   The path to the service's build context directory, relative to the Sailr project root. This directory should typically contain the `Dockerfile` (or the specified `dockerfile`) and all source code needed to build the image.
*   If this field is present, Sailr will attempt to build an image for this service using the selected build backend. If absent, Sailr assumes it's a pre-built image to be pulled from a registry.
*   Example: `build = "./services/backend-api/"`

#### `dockerfile` (string)
*   **Optional**
*   The path to the Dockerfile, relative to the `build` context directory.
*   Defaults to `Dockerfile` at the root of the `build` path.
*   Example: `dockerfile = "path/to/custom.Dockerfile"`

#### `run_parallel` (string or array of strings)
*   **Optional**
*   A shell command or list of shell commands to run in parallel during the build phase for this service. These commands are executed within the `build` context directory.
*   These commands run concurrently within the service when using the runkernel backend. Backend-level inter-service parallelism depends on the selected build backend.
*   Example: `run_parallel = "npm install && npm run build"`
*   Example: `run_parallel = ["yarn install", "yarn build:assets"]`

#### `run_synchronous` (string or array of strings)
*   **Optional**
*   A shell command or list of shell commands to run synchronously during the build phase for this service. These commands are executed within the `build` context directory.
*   These commands run sequentially for this service.
*   Example: `run_synchronous = "./scripts/prepare_data.sh"`

#### `before` (string or array of strings)
*   **Optional**
*   A shell command or list of shell commands to run *before* the main build steps (`run_parallel`, `run_synchronous`, Docker build) for this service. Executed within the `build` context directory.
*   Example: `before = "./scripts/pre_build_checks.sh"`

#### `before_synchronous` (string or array of strings)
*   **Optional**
*   A shell command or list of shell commands to run synchronously *before* other build steps (including `before`) for this service. Executed within the `build` context directory.
*   This hook provides a way to ensure certain prerequisite tasks are completed sequentially before any other build activity for the service.
*   Example: `before_synchronous = "echo 'Starting critical synchronous pre-build tasks'"`

#### `after` (string or array of strings)
*   **Optional**
*   A shell command or list of shell commands to run *after* all other build steps (including the Docker image build) have successfully completed for this service. Executed within the `build` context directory.
*   Useful for cleanup tasks, notifications, or pushing images to a staging registry.
*   Example: `after = "./scripts/post_build_cleanup.sh"`

**Build Execution Order:**
For a single service, Sailr runs build hooks in this order:
1.  `before_synchronous` commands (sequentially)
2.  `before` commands
3.  `run_parallel` commands
4.  `run_synchronous` commands (sequentially for this service)
5.  Docker image build (using the specified `build` context and `dockerfile`).
6.  `after` commands

Inter-service ordering follows service build dependencies. Global `before_all` and `after_all` hooks are configured in the top-level `[build]` table.

## Environment Variables (`[[environment_variables]]`)

This is an array of tables, where each table defines an environment variable that will be available during the manifest templating process. These variables are accessible in your templates using `{{variable_name}}`.

Each environment variable entry has the following properties:

### `name` (string)
*   **Required**
*   The name of the environment variable (the key used in templates).
*   Example: `name = "API_ENDPOINT"`

### `value` (string)
*   **Required**
*   The value to be assigned to the environment variable. This value will replace the corresponding `{{name}}` placeholder in your templates.
*   Example: `value = "https://api.example.com/v1"`

---

This guide should provide a solid understanding of how to configure your Sailr environments using `config.toml`. For information on how these configurations are used in practice, refer to the [Getting Started Tutorial](../getting-started/tutorial.md).
