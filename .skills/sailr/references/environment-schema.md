# Environment Configuration (`config.toml`) Schema 0.5.0

The environment file `k8s/environments/<env>/config.toml` strictly follows Schema `0.5.0`.

## Top-Level Properties
| Parameter | Type | Required? | Description | Default / Example |
| :--- | :--- | :--- | :--- | :--- |
| `schema_version` | string | **Required** | Must be `"0.5.0"` | `"0.5.0"` |
| `name` | string | **Required** | Environment name (unless using `extends`) | `"develop"` |
| `domain` | string | **Required** | Base domain for Ingress/service URLs | `"dev.local"` |
| `extends` | string | Optional | Parent environment name to extend/inherit from | `"develop"` |
| `log_level` | string | Optional | Log verbosity (`TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR`) | `"INFO"` |
| `default_replicas` | integer | Optional | Default replica count | `1` |
| `registry` | string | Optional | Container image registry | `"docker.io"` |

## Services (`[[service]]`)
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

## Environment Variables (`[[environment_variables]]`)
Injects variables into templates during manifest generation:
```toml
[[environment_variables]]
name = "DATABASE_URL"
value = "postgres://db.dev.local:5432/main"

[[environment_variables]]
name = "LOG_FORMAT"
value = "json"
```

## Layered / Inherited Environments
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
