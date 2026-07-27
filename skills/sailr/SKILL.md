---
name: sailr
description: Understand and operate Sailr, an environment management CLI for Kubernetes. Use when initializing projects/environments, adding services, configuring config.toml and sailr.workflow.toml, building container images, generating k8s manifests, deploying to clusters, running workflow profiles, or linting/migrating sailr configurations.
---

# Sailr Agent Skill Guide

This skill provides comprehensive instructions, schemas, CLI commands, workflow patterns, and best practices for working with **Sailr** — an environment and Kubernetes management CLI.

---

## 1. Overview & Core Philosophy

Sailr simplifies managing multi-service Kubernetes applications across local, staging, and production environments. It automates container building, manifest generation from templates, and cluster deployments.

### Key Architecture Concepts
* **Environment**: A deployment target (e.g., `local`, `develop`, `staging`, `production`) configured via `k8s/environments/<env_name>/config.toml`.
* **Service**: A microservice or application component defined in `config.toml` as `[[service]]`, backed by templates in `k8s/templates/<service_name>/`.
* **Templates**: Handlebars-like YAML templates (`deployment.yaml`, `service.yaml`, `configmap.yaml`, Ingress, etc.) using placeholder replacement.
* **Build Engines**: Pluggable container build backends — `roomservice` (default) and `runkernel` (experimental workflow engine).
* **Workflow Engine**: Orchestrates multi-step CI/CD profiles defined in `sailr.workflow.toml`.

---

## 2. Directory & Project Structure

A typical Sailr project adheres to the following directory layout:

```
.
├── sailr.workflow.toml             # (Optional) CI/CD & workflow profile definitions
├── k8s/
│   ├── environments/
│   │   ├── develop/
│   │   │   └── config.toml        # Environment configuration
│   │   └── staging/
│   │       └── config.toml        # Derived / layered configuration
│   ├── templates/
│   │   ├── api/
│   │   │   ├── deployment.yaml   # K8s manifest templates
│   │   │   ├── service.yaml
│   │   │   └── configmap.yaml
│   │   └── web/
│   │       └── ...
│   └── generated/
│       └── develop/               # Manifests output by 'sailr generate' (Do not edit directly)
│           ├── api.yaml
│           └── web.yaml
└── .sailr/                        # Sailr internal project cache (e.g., runkernel cache)
```

---

## 3. Configuration Reference (`config.toml`)

The environment file `k8s/environments/<env>/config.toml` strictly follows **Schema `0.5.0`**.

### 3.1 Top-Level Properties
| Parameter | Type | Required? | Description | Default / Example |
| :--- | :--- | :--- | :--- | :--- |
| `schema_version` | string | **Required** | Must be `"0.5.0"` | `"0.5.0"` |
| `name` | string | **Required** | Environment name (unless using `extends`) | `"develop"` |
| `domain` | string | **Required** | Base domain for Ingress/service URLs | `"dev.local"` |
| `extends` | string | Optional | Parent environment name to extend/inherit from | `"develop"` |
| `log_level` | string | Optional | Log verbosity (`TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR`) | `"INFO"` |
| `default_replicas` | integer | Optional | Default replica count | `1` |
| `registry` | string | Optional | Container image registry | `"docker.io"` |

### 3.2 Global Build Policy (`[build]`)
```toml
[build]
engine = "runkernel"      # "roomservice" (default) or "runkernel"
fail_fast = false         # Roomservice policy; runkernel settles active siblings
max_parallelism = 4       # Concurrency level
before_all = "echo 'Starting builds'"
after_all = "echo 'Builds completed'"
```

### 3.3 Services (`[[service]]`)
Defines application components and optional build steps:
```toml
[[service]]
name = "api"
version = "1.2.3"
path = "api"             # Relative to k8s/templates/ (defaults to name)
namespace = "default"    # Target k8s namespace (defaults to env name)

[service.build]
path = "./services/api"  # Build context path relative to root
dockerfile = "Dockerfile"
before = "echo 'Preparing build'"
run_parallel = "npm run test"
run_synchronous = "npm run build"
after = "echo 'Build complete'"
finally = "echo 'Clean up after settlement'"
ignore_cache = ["*.md", "tests/**"]
```

`ignoreCache` is accepted as an alias. Runkernel resolves and sorts exact input
files; ignored patterns are relative to the build path. `--force` bypasses
cache reads and writes for executable translated service tasks without deleting
the prior cache. Global hooks and aggregates are never cached, and global hooks
are suppressed when no selected service is dirty. Service `finally` commands
run exactly once after active siblings settle, in stable reverse dependency
order.

### 3.3 Deployment Policy

Cluster security is explicit and never inferred from an environment name:

```toml
[deployment_policy]
required_approval = "signature" # none | external | signature
```

### 3.4 Environment Variables (`[[environment_variables]]`)
Injects variables into templates during manifest generation:
```toml
[[environment_variables]]
name = "DATABASE_URL"
value = "postgres://db.dev.local:5432/main"

[[environment_variables]]
name = "LOG_FORMAT"
value = "json"
```

### 3.5 Layered / Inherited Environments
Derived environments can inherit and override settings:
```toml
# k8s/environments/staging/config.toml
schema_version = "0.5.0"
extends = "develop"
domain = "staging.example.com"

[[environment_variables]]
name = "LOG_FORMAT"
value = "text"
```

---

## 4. Template Placeholder Reference

Sailr replaces `{{ placeholder }}` in `k8s/templates/` during generation. Available template variables include:

| Variable Placeholder | Available Value |
| :--- | :--- |
| `{{ name }}` or `{{ env_name }}` | Environment name (`config.toml` `name`) |
| `{{ domain }}` | Base domain (`config.toml` `domain`) |
| `{{ registry }}` | Target container registry (`config.toml` `registry`) |
| `{{ default_replicas }}` | Default replica count |
| `{{ service_name }}` | Name of the current service (`[[service]].name`) |
| `{{ service_version }}` | Version / image tag of the service (`[[service]].version`) |
| `{{ service_namespace }}` | Target k8s namespace for the service |
| `{{ <ENV_VAR_NAME> }}` | Any variable defined in `[[environment_variables]]` |

---

## 5. Workflow Profiles (`sailr.workflow.toml`)

`sailr.workflow.toml` defines reusable CI/CD pipeline profiles executed via `sailr workflow run <profile>`.

```toml
[workflow.ci-check]
environment = "develop"
mode = "check"
interactive = false
build = "plan"
generate = "disabled"
deploy = "disabled"
report = "text"

[workflow.staging-deploy]
environment = "staging"
mode = "deploy"
interactive = false
build = "run"
generate = "run"
deploy = "run"
deploy_context = "staging-cluster"
namespace = "default"
approval = "external"
apply = true
report = "both"
```

For an immutable, signed deployment:

```toml
[workflow.production]
environment = "production"
mode = "deploy"
interactive = false
build = "disabled"
generate = "run"
deploy = "run"
deploy_context = "production-cluster"
namespace = "app"
approval = "signature"
apply = true
report = "both"

[workflow.production.signature]
trusted_public_key = "<base64 raw 32-byte Ed25519 public key>"
```

Sailr renders only the key's `sha256:<hex>` fingerprint. The first run writes
`.sailr/audit/<profile>/deployment-plan.json` and reports
`awaiting_signature` without cluster mutation. Sign the exact ASCII message
`sailr-deployment-plan-v1:<plan_hash>` outside Sailr, set only
`DEPLOY_APPROVAL_SIG` to the base64 raw 64-byte signature, and retry. The
bundle and gate are uncached; deterministic earlier work may be `[CACHE]`.
Planning and deployment use the same in-memory canonical JSON resources and do
not reopen generated YAML.

---

## 6. CLI Command Reference

### 6.1 Project & Service Initialization
* **Initialize a new environment**:
  ```bash
  sailr init --name <ENV_NAME> [--engine roomservice|runkernel]
  ```
* **Add a new service**:
  ```bash
  sailr add service <SERVICE_NAME> --type web-app|worker|database
  ```
  *(Creates `k8s/templates/<SERVICE_NAME>/` and updates default environment `config.toml`)*

### 6.2 Configuration Utilities
* **Lint configuration**:
  ```bash
  sailr lint [--config <PATH>]
  ```
* **Migrate configuration to schema 0.5.0**:
  ```bash
  sailr migrate --name <ENV_NAME> [--engine roomservice|runkernel]
  ```
* **Bump service version**:
  ```bash
  sailr bump --service <SERVICE_NAME> --version <NEW_VERSION> [--env <ENV_NAME>]
  ```

### 6.3 Core Operations (Build, Generate, Deploy, Go)
* **Build container images**:
  ```bash
  sailr build --name <ENV_NAME> [--force] [--engine roomservice|runkernel] [--ignore <SVC1,SVC2>]
  ```
* **Generate Kubernetes manifests**:
  ```bash
  sailr generate --name <ENV_NAME> [--only <SVC1,SVC2>] [--ignore <SVC3>]
  ```
* **Deploy to Kubernetes context**:
  ```bash
  sailr deploy --name <ENV_NAME> --context <K8S_CONTEXT> [--strategy Rolling|Restart]
  ```
* **Combined All-in-One (Build + Generate + Deploy)**:
  ```bash
  sailr go --name <ENV_NAME> --context <K8S_CONTEXT> [--force] [--strategy Rolling|Restart]
  ```

### 6.4 Infrastructure & Interactive Commands
* **Spin up infrastructure**: `sailr infra up <ENV_NAME> --provider Local`
* **Tear down infrastructure**: `sailr infra down --name <ENV_NAME>`
* **Launch Interactive TUI mode**: `sailr interactive --context <K8S_CONTEXT> --name <ENV_NAME>`

### 6.5 Workflow Management (`sailr workflow`)
* **List workflow profiles**: `sailr workflow list`
* **Show profile configuration**: `sailr workflow show <PROFILE>`
* **Plan a workflow run**: `sailr workflow plan <PROFILE> [--format text|json]`
* **Run a workflow profile**: `sailr workflow run <PROFILE> [--dry-run] [--apply]`
* **Export workflow dependency graph**: `sailr workflow graph <PROFILE> --format mermaid`
* **Generate CI configuration**: `sailr workflow generate-ci <PROFILE> --provider github`

---

## 7. Operational Workflows for AI Agents

When acting on behalf of a user to perform tasks with Sailr, follow these step-by-step procedures:

### Workflow A: Adding a New Service to an Application
1. Run `sailr add service <service_name> --type web-app`.
2. Inspect `k8s/templates/<service_name>/` and adjust `deployment.yaml`, `service.yaml`, and `configmap.yaml` as needed.
3. Edit `k8s/environments/<env>/config.toml` to specify `[service.build]` paths and commands if container image building is required.
4. Run `sailr lint` to verify configuration validity.

### Workflow B: Updating & Re-deploying an Environment
1. Bump the target service version using `sailr bump --service <service_name> --version <version>`.
2. Generate manifests using `sailr generate --name <env_name>` and inspect the output in `k8s/generated/<env_name>/`.
3. Deploy changes using `sailr deploy --name <env_name> --context <cluster_context> --strategy Rolling` or use `sailr go`.

### Workflow C: Running CI Checks or Pipelines
1. Inspect available workflow profiles with `sailr workflow list`.
2. Plan execution with `sailr workflow plan <profile_name>`.
3. Run the workflow with `sailr workflow run <profile_name>`.

For signature approval, expect two runs: the unsigned run creates the audit
artifact and report; an external operator signs the reported plan hash; the
retry supplies only `DEPLOY_APPROVAL_SIG`. Never place a private key in Sailr.

---

## 8. Best Practices & Critical Rules

> [!IMPORTANT]
> **Never modify `k8s/generated/` manually.** The `k8s/generated/` directory is ephemeral and overwritten whenever `sailr generate` or `sailr go` runs. Always modify templates in `k8s/templates/` or configuration in `k8s/environments/`.

> [!NOTE]
> **Build Engines**: Use `runkernel` engine (`--engine runkernel` or `[build].engine = "runkernel"`) for modern, workflow-backed cached builds. Cache files are stored in `.sailr/cache/build`.

> [!CAUTION]
> **Schema Integrity**: Always ensure `config.toml` uses `schema_version = "0.5.0"`. Avoid legacy `service_whitelist` configurations and migrate using `sailr migrate`.

> [!CAUTION]
> **Rollback scope**: Sailr journals only successful Kubernetes mutations.
> Partial failure restores updates and deletes creates in reverse order; later
> post-deploy failure can reuse the completed journal. Hook side effects are
> observable but not reversible. Rollback errors are aggregated and fail the
> workflow.
