# Template Placeholder Reference

Sailr replaces `{{ placeholder }}` in `k8s/templates/` during generation. Available variables:

| Variable Placeholder | Available Value |
| :--- | :--- |
| `{{ name }}` or `{{ env_name }}` | Environment name (`config.toml` `name`) |
| `{{ domain }}` | Base domain (`config.toml` `domain`) |
| `{{ registry }}` | Target container registry (`config.toml` `registry`) |
| `{{ default_replicas }}` | Default replica count |
| `{{ service_name }}` | Name of the current service (`[[service]].name`) |
| `{{ service_version }}` | Explicit `[[service]].version`, or the derived immutable build-fingerprint tag when a build-backed service omits it |
| `{{ service_image }}` | Complete deployable image reference for build-backed services. Normal and legacy generation use the explicit version when present or the derived immutable tag when omitted; release preparation injects the promoted digest reference. Portable release workload templates for services with `build` must use this variable. |
| `{{ service_namespace }}` | Target k8s namespace for the service |
| `{{ <ENV_VAR_NAME> }}` | Any variable defined in `[[environment_variables]]` |

Services without a `build` configuration are external dependencies rather than promotion
artifacts. Keep their vendor repository explicit, such as
`image: emqx/nanomq:{{service_version}}`; do not rewrite them to `{{service_image}}`.
Treat an external workload that omits `{{service_version}}` as a warning, not a release-blocking
error.

An explicit version is authoritative across build, push, and generation. Omit it only for a
build-backed service when Sailr should derive the immutable fingerprint tag. Do not omit the
version for an external service because Sailr has no build fingerprint from which to derive one.
