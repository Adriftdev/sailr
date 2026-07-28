# Template Placeholder Reference

Sailr replaces `{{ placeholder }}` in `k8s/templates/` during generation. Available variables:

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
