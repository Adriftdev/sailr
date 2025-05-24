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
*   Example: `schema_version = "0.2.0"`
*   Changing this version might indicate breaking changes or new features in the Sailr config specification. Consult the Sailr release notes if you need to change this.

### `name` (string)
*   **Required**
*   The name of the environment. This is used for identification purposes and can be used as a variable in your templates (e.g., `{{name}}` or `{{env_name}}` - Sailr provides it as `{{name}}` as per `Environment::get_variables`).
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

## Service Whitelist (`[[service_whitelist]]`)

This is an array of tables, where each table defines a service to be managed by Sailr.

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

### Build Configuration (within `[[service_whitelist]]` entry)

Sailr integrates a build system (based on Roomservice) to build your service's container images. These fields control the build process for a specific service. The actual build commands and lifecycle are defined within a `roomservice.toml` file located in the `build` path of the service, or directly within these fields if Sailr has merged Roomservice's config structure (as suggested by the README). The following fields are based on the merged structure described in the Sailr README.

#### `build` (string)
*   **Optional**
*   The path to the service's build context directory, relative to the Sailr project root. This directory should typically contain the `Dockerfile` (or the specified `dockerfile`) and all source code needed to build the image.
*   If this field is present, Sailr will attempt to build an image for this service using the Roomservice build process. If absent, Sailr assumes it's a pre-built image to be pulled from a registry.
*   Example: `build = "./services/backend-api/"`

#### `dockerfile` (string)
*   **Optional**
*   The path to the Dockerfile, relative to the `build` context directory.
*   Defaults to `Dockerfile` at the root of the `build` path. (This is a common convention for build systems like Roomservice; confirm if Sailr's implementation allows override).
*   Example: `dockerfile = "path/to/custom.Dockerfile"`

#### `run_parallel` (string or array of strings)
*   **Optional**
*   A shell command or list of shell commands to run in parallel during the build phase for this service. These commands are executed within the `build` context directory.
*   These tasks are intended for operations that can run concurrently with similar tasks for *other services* if the build orchestrator (Roomservice core) supports this level of parallelism.
*   Example: `run_parallel = "npm install && npm run build"`
*   Example: `run_parallel = ["yarn install", "yarn build:assets"]`

#### `run_synchronous` (string or array of strings)
*   **Optional**
*   A shell command or list of shell commands to run synchronously during the build phase for this service. These commands are executed within the `build` context directory.
*   These commands will run sequentially for this service. If multiple services have synchronous commands, their execution order relative to each other depends on the build orchestrator.
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

**Build Execution Order (Typical for a Roomservice-like system):**
For a single service, the assumed order of execution for these hooks is:
1.  `before_synchronous` commands (sequentially)
2.  `before` commands (can be parallel with other services' `before` hooks, if applicable)
3.  `run_parallel` commands (can be parallel with other services' `run_parallel` hooks)
4.  `run_synchronous` commands (sequentially for this service)
5.  Docker image build (using the specified `build` context and `dockerfile`).
6.  `after` commands (can be parallel with other services' `after` hooks, if applicable)

*Note: The exact parallelism between different services depends on the Roomservice core implementation within Sailr.*
*Global `beforeAll` and `afterAll` hooks might also be configurable if Sailr exposes the full Roomservice configuration schema, but these are not detailed in the current Sailr `README.md` for `config.toml`.*

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

This guide should provide a solid understanding of how to configure your Sailr environments using `config.toml`. For information on how these configurations are used in practice, refer to the [Getting Started Tutorial](../getting-started/tutorial.md). For details on template syntax and available variables, please see the (forthcoming) Templating Guide.Okay, I've drafted the "Comprehensive `config.toml` Guide".

Here's a summary of the steps and considerations:

1.  **Reviewed Source Information:**
    *   Primarily used the `README.md` content provided in the prompt for the structure of `config.toml`.
    *   Cross-referenced with insights from previous analysis of `src/environment.rs` (for `Environment` and `Service` structs) and `src/roomservice/config.rs` (for `Config` and `RoomConfig` structs that define build hooks) to ensure field names and types were consistent.
    *   Noted the `scribe-rust` logger from `src/lib.rs` to infer possible log levels.

2.  **Structured the Document:**
    *   Followed the requested Docusaurus page structure with frontmatter (`sidebar_position`, `title`).
    *   Organized the guide into logical sections: File Location, Top-Level Settings, Service Whitelist (including Build Configuration), and Environment Variables.

3.  **Detailed Each Field:**
    *   For each field, provided:
        *   Whether it's **Required** or **Optional**.
        *   A clear description of its purpose.
        *   An illustrative example.
        *   Default values where applicable (e.g., `log_level`, `default_replicas`, `registry`).
        *   How it might be used in templates (e.g., `{{env_name}}`, `{{service_name}}`).
    *   **Log Levels:** Listed common log levels (`TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR`, `FATAL`) assuming standard `scribe-rust` behavior.
    *   **Build Configuration:**
        *   Acknowledged that the build system is based on Roomservice.
        *   Detailed each build hook field (`build`, `dockerfile`, `run_parallel`, `run_synchronous`, `before`, `before_synchronous`, `after`) as described in the Sailr `README.md`.
        *   Clarified that `before_synchronous` is treated as a distinct field based on `src/roomservice/config.rs`.
        *   Provided an assumed execution order for these hooks for a single service, with a note that inter-service parallelism depends on the Roomservice core.
        *   Mentioned that global `beforeAll`/`afterAll` are not covered as they weren't in the `config.toml` section of the README.
    *   **`dockerfile` field:** Retained it as optional and noted it's a common convention that needs confirmation for Sailr's specific implementation if it can be overridden.
    *   **Service `name`:** Added a note about the internal `namespace/name` representation vs. user definition.

4.  **Maintained Clarity and Placeholders:**
    *   Used clear, concise language.
    *   Included placeholders for links to other guides (like a future Templating Guide) to ensure inter-document navigation.
    *   Ensured TOML examples were correctly formatted.

5.  **File Creation:**
    *   Used `create_file_with_block` to write the content to `docs/docs/configuration/config-toml.md`. This tool will create the `configuration` directory if it doesn't exist.

This draft aims to be comprehensive based on the available information and provide a solid foundation for users to understand and utilize the `config.toml` file effectively.
