---
sidebar_position: 3
title: CLI Command Reference
---

# Sailr CLI Command Reference

This page provides a comprehensive reference for all Sailr Command Line Interface (CLI) commands.

## Global Options

Sailr does not currently have global options that apply to all commands (e.g., `--verbose`). Options are specific to each command or subcommand.

## Main Commands

Sailr commands generally follow the pattern `sailr [COMMAND] [SUBCOMMAND] [ARGUMENTS] [OPTIONS]`.

---

### `sailr init`

Initializes a new Sailr environment, creating its directory structure (e.g., `./k8s/environments/<NAME>`) and a default `config.toml` file.

*   **Usage:** `sailr init [OPTIONS] --name <NAME>`
*   **Options:**
    *   `-n, --name <NAME>`: (Required) The name for the new environment. This will also be the directory name created.
    *   `-c, --config-template <CONFIG_TEMPLATE_PATH>`: Path to a custom `config.toml` template to use instead of the default one.
    *   `-r, --registry <DEFAULT_REGISTRY>`: Default container registry to use for images in this environment (e.g., `docker.io/myorg`).
    *   `-p, --provider <PROVIDER>`: Infrastructure provider to use for generating default infrastructure configurations.
        *   Possible values: `Local`, `Aws`, `Gcp`. (Note: `Aws` and `Gcp` provider functionalities might be placeholders or under development).
        *   Defaults to `Local` if infrastructure options are used without specifying a provider.
    *   `-i, --infra-templates <INFRA_TEMPLATE_PATH>`: Path to custom infrastructure templates to use instead of provider defaults.
    *   `-R, --region <REGION>`: Cloud provider region to use (if applicable to the chosen provider).
*   **Examples:**
    ```bash
    # Initialize a new environment named "dev-environment"
    sailr init --name dev-environment

    # Initialize with a custom registry and AWS provider settings
    sailr init --name staging --registry quay.io/my-company --provider Aws --region us-east-1
    ```
*   **Note on Default Service:** The `sailr init` command also creates a default "sample-app" service. This includes generating basic Kubernetes manifest templates (Deployment, Service, ConfigMap) in `k8s/templates/sample-app/` and adding a corresponding service entry to the new environment's `config.toml`. This makes the newly initialized environment immediately runnable and provides a quick way to demonstrate Sailr's capabilities.

---

### `sailr add service`

Adds a new service to your Sailr project. This involves generating boilerplate Kubernetes manifest templates and updating the environment configuration.

*   **Usage:** `sailr add service <SERVICE_NAME> --type <APP_TYPE>`
*   **Arguments & Options:**
    *   `<SERVICE_NAME>`: (Required) The name for the new service (e.g., `my-api`, `frontend-app`). This name will be used for the template directory and in the service configuration.
    *   `-t, --type <APP_TYPE>`: (Required) Specifies the type of application (e.g., `web-app`, `worker`, `database`). This can influence the structure and content of the generated templates.
*   **Actions Performed:**
    *   Creates a new directory `k8s/templates/<SERVICE_NAME>/`.
    *   Generates the following Kubernetes manifest template files within this directory:
        *   `deployment.yaml`
        *   `service.yaml`
        *   `configmap.yaml`
    *   Adds a new service entry to the `k8s/environments/develop/config.toml` file (assuming "develop" is the current or default environment for this operation). This entry allows Sailr to manage and deploy the new service.
*   **Example:**
    ```bash
    # Add a new web application service named "user-api"
    sailr add service user-api --type web-app
    ```

---

### `sailr completions`

Generates shell completion scripts for various shells.

*   **Usage:** `sailr completions <SHELL>`
*   **Arguments:**
    *   `<SHELL>`: (Required) The shell to generate completions for.
        *   Possible values: `bash`, `zsh`, `fish`, `powershell`, `elvish`.
*   **Examples:**
    ```bash
    # Generate bash completions and source them for the current session
    source <(sailr completions bash)

    # Generate zsh completions and save to a file (e.g., for Oh My Zsh)
    # mkdir -p ~/.oh-my-zsh/custom/completions
    # sailr completions zsh > ~/.oh-my-zsh/custom/completions/_sailr
    ```
    *(Refer to the [Installation Guide](./getting-started/installation.md#shell-completions) for more detailed setup instructions.)*

---

### `sailr infra`

Manages underlying infrastructure for environments (e.g., local Kubernetes cluster setup via OpenTofu/Terraform).

#### `sailr infra up`

Sets up or updates the infrastructure for an environment based on its configuration.

*   **Usage:** `sailr infra up [OPTIONS] <NAME>`
*   **Arguments:**
    *   `<NAME>`: (Required) Name of the environment whose infrastructure needs to be set up/updated.
*   **Options:**
    *   `--provider <PROVIDER>`: Infrastructure provider to use.
        *   Possible values: `Local`, `Aws`, `Gcp`.
    *   `--registry <DEFAULT_REGISTRY>`: Default container registry to configure within the infrastructure (if applicable).
    *   `--infra-templates <INFRA_TEMPLATE_PATH>`: Path to custom infrastructure templates.
    *   `--region <REGION>`: (Note: `CreateArgs` in `cli.rs` uses short `-r` for region, while `InitArgs` uses `-R`. For consistency in docs, using long form. Actual CLI might differ if short flags clash.) Cloud provider region.
*   **Example:**
    ```bash
    sailr infra up dev-environment --provider Local
    ```

#### `sailr infra down`

Tears down the infrastructure for an environment.

*   **Usage:** `sailr infra down --name <NAME>`
*   **Options:**
    *   `-n, --name <NAME>`: (Required) Name of the environment whose infrastructure needs to be torn down.
*   **Example:**
    ```bash
    sailr infra down --name dev-environment
    ```

---

### `sailr deploy`

Deploys an existing, generated environment to a Kubernetes cluster. This command applies the manifests found in `./k8s/generated/<NAME>/`.

*   **Usage:** `sailr deploy --name <NAME> --context <CONTEXT> [--strategy <STRATEGY>]`
*   **Options:**
    *   `-n, --name <NAME>`: (Required) Name of the environment to deploy.
    *   `-c, --context <CONTEXT>`: (Required) The Kubernetes cluster context to deploy to (as listed in your kubeconfig).
    *   `--strategy <STRATEGY>`: Specifies the deployment strategy to use.
        *   Possible values: `Restart`, `Rolling`.
        *   Defaults to `Rolling`.
        *   `Restart`: Before applying new manifests, this strategy first deletes any existing Kubernetes Deployments that are defined in the environment's generated files. This ensures that associated pods are cleanly restarted with the new version.
        *   `Rolling`: This strategy applies the new manifests and relies on Kubernetes to perform a standard rolling update if the Deployment resources are configured for it (this is the default update strategy for Kubernetes Deployments). Sailr does not perform any explicit deletions of resources with this strategy.
*   **Example:**
    ```bash
    # Deploy with the default Restart strategy
    sailr deploy --name production --context prod-cluster

    # Deploy using a Rolling update strategy
    sailr deploy --name staging --context stage-cluster --strategy Rolling
    ```

---

### `sailr generate`

Generates Kubernetes deployment manifests for an environment based on its `config.toml` and templates. Manifests are saved to `./k8s/generated/<NAME>/`. This command does not deploy to the cluster.

*   **Usage:** `sailr generate [OPTIONS] --name <NAME>`
*   **Options:**
    *   `-n, --name <NAME>`: (Required) Name of the environment to generate manifests for.
    *   `--only <SERVICES>`: Comma-separated list of service names (e.g., `service1,service2`) to generate. If provided, only these services defined in `config.toml` will be processed.
    *   `--ignore <SERVICES>`: Comma-separated list of service names to ignore. These services will not be processed.
*   **Examples:**
    ```bash
    # Generate manifests for all services in the "staging" environment
    sailr generate --name staging

    # Generate manifests only for "api-service" and "worker-service"
    sailr generate --name staging --only api-service,worker-service

    # Generate manifests for all services except "legacy-app"
    sailr generate --name staging --ignore legacy-app
    ```

---

### `sailr build`

Builds container images for services defined in an environment's `config.toml` that have a `build` configuration.

*   **Usage:** `sailr build [OPTIONS] --name <NAME>`
*   **Options:**
    *   `-n, --name <NAME>`: (Required) Name of the environment whose services need building.
    *   `-f, --force`: Force all services with a `build` configuration to rebuild, ignoring any cached build status or previous image digests.
    *   `-i, --ignore <SERVICES>`: Comma-separated list of service names to ignore during the build process.
*   **Examples:**
    ```bash
    # Build all services in the "dev" environment that have build configurations
    sailr build --name dev

    # Force rebuild all services in "dev", ignoring "legacy-service"
    sailr build --name dev --force --ignore legacy-service
    ```

---

### `sailr go`

A comprehensive command that performs a sequence of actions:
1.  Builds container images for services (respecting `--force`, `--ignore`, `--only`).
2.  Generates Kubernetes manifests (respecting `--only`, `--ignore` based on the services selected for building/processing).
3.  Deploys the generated manifests to the specified Kubernetes cluster using the chosen deployment strategy.

*   **Usage:** `sailr go [OPTIONS] --name <NAME> --context <CONTEXT> [--strategy <STRATEGY>]`
*   **Options:**
    *   `-n, --name <NAME>`: (Required) Name of the environment.
    *   `-c, --context <CONTEXT>`: (Required) The Kubernetes cluster context to deploy to.
    *   `-f, --force`: Force rebuild of all images during the build phase.
    *   `-i, --ignore <SERVICES>`: Comma-separated list of service names to ignore for build and manifest generation phases.
    *   `--only <SERVICES>`: Comma-separated list of service names to process for build and manifest generation phases.
    *   `--strategy <STRATEGY>`: Specifies the deployment strategy to use for the deployment phase.
        *   Possible values: `Restart`, `Rolling`.
        *   Defaults to `Rolling`.
        *   `Restart`: Ensures a clean redeployment by first deleting existing Kubernetes Deployments (managed by Sailr for this environment, based on generated manifests) before applying the new ones.
        *   `Rolling`: Relies on Kubernetes' standard rolling update mechanism based on the manifest configurations.
*   **Example:**
    ```bash
    # Run 'go' with the default Restart strategy for deployment, processing only api and frontend
    sailr go --name staging --context stage-cluster --force --only api,frontend

    # Run 'go' using a Rolling update strategy for deployment
    sailr go --name production --context prod-cluster --strategy Rolling
    ```

---

### `sailr k8s`

Provides commands to interact directly with Kubernetes resources within a cluster. These commands are useful for inspecting or managing resources related to Sailr environments.

#### `sailr k8s pod`

Manage pods within a Kubernetes cluster.

*   **`sailr k8s pod get --context <CONTEXT>`**
    *   Lists pods in the default namespace of the specified Kubernetes context.
    *   **Options:**
        *   `-c, --context <CONTEXT>`: (Required) Kubernetes context to use.
    *   **Example:** `sailr k8s pod get --context my-dev-cluster`

*   **`sailr k8s pod delete [OPTIONS] --name <POD_NAME> --context <CONTEXT>`**
    *   Deletes a specific pod by name.
    *   **Options:**
        *   `--name <POD_NAME>`: (Required) Name of the pod to delete. (Note: `cli.rs` defines `short = 'n'` for this).
        *   `-c, --context <CONTEXT>`: (Required) Kubernetes context to use.
        *   `--namespace <NAMESPACE>`: Namespace of the pod. If omitted, uses the default namespace from the Kubernetes context. (Note: `cli.rs` also defines `short = 'n'` for this. Prioritize long flags in examples due to potential short flag conflict if not automatically resolved by `clap`).
    *   **Example:** `sailr k8s pod delete --name my-app-pod-123 --context my-dev-cluster --namespace my-application`

*   **`sailr k8s pod delete-all --namespace <NAMESPACE> --context <CONTEXT>`**
    *   Deletes all pods in a specified namespace.
    *   **Options:**
        *   `-n, --namespace <NAMESPACE>`: (Required) Namespace from which to delete all pods.
        *   `-c, --context <CONTEXT>`: (Required) Kubernetes context to use.
    *   **Example:** `sailr k8s pod delete-all --namespace my-application --context my-dev-cluster`

#### `sailr k8s deployment`

Manage deployments within a Kubernetes cluster.

*   **`sailr k8s deployment get --context <CONTEXT>`**
    *   Lists deployments in the default namespace of the specified Kubernetes context.
    *   **Options:**
        *   `-c, --context <CONTEXT>`: (Required) Kubernetes context to use.
    *   **Example:** `sailr k8s deployment get --context my-dev-cluster`

*   **`sailr k8s deployment delete [OPTIONS] --name <DEPLOYMENT_NAME> --context <CONTEXT>`**
    *   Deletes a specific deployment by name.
    *   **Options:**
        *   `--name <DEPLOYMENT_NAME>`: (Required) Name of the deployment to delete.
        *   `-c, --context <CONTEXT>`: (Required) Kubernetes context to use.
        *   `--namespace <NAMESPACE>`: Namespace of the deployment. If omitted, uses the default namespace from the Kubernetes context.
    *   **Example:** `sailr k8s deployment delete --name my-app-deployment --context my-dev-cluster --namespace my-application`

*   **`sailr k8s deployment delete-all --namespace <NAMESPACE> --context <CONTEXT>`**
    *   Deletes all deployments in a specified namespace.
    *   **Options:**
        *   `-n, --namespace <NAMESPACE>`: (Required) Namespace from which to delete all deployments.
        *   `-c, --context <CONTEXT>`: (Required) Kubernetes context to use.
    *   **Example:** `sailr k8s deployment delete-all --namespace my-application --context my-dev-cluster`

#### `sailr k8s service`

Manage services within a Kubernetes cluster.

*   **`sailr k8s service get --context <CONTEXT>`**
    *   Lists services in the default namespace of the specified Kubernetes context.
    *   **Options:**
        *   `-c, --context <CONTEXT>`: (Required) Kubernetes context to use.
    *   **Example:** `sailr k8s service get --context my-dev-cluster`

*   **`sailr k8s service delete [OPTIONS] --name <SERVICE_NAME> --context <CONTEXT>`**
    *   Deletes a specific service by name.
    *   **Options:**
        *   `--name <SERVICE_NAME>`: (Required) Name of the service to delete.
        *   `-c, --context <CONTEXT>`: (Required) Kubernetes context to use.
        *   `--namespace <NAMESPACE>`: Namespace of the service. If omitted, uses the default namespace from the Kubernetes context.
    *   **Example:** `sailr k8s service delete --name my-app-service --context my-dev-cluster --namespace my-application`

*   **`sailr k8s service delete-all --namespace <NAMESPACE> --context <CONTEXT>`**
    *   Deletes all services in a specified namespace.
    *   **Options:**
        *   `-n, --namespace <NAMESPACE>`: (Required) Namespace from which to delete all services.
        *   `-c, --context <CONTEXT>`: (Required) Kubernetes context to use.
    *   **Example:** `sailr k8s service delete-all --namespace my-application --context my-dev-cluster`

---
