---
title: Immutable release approval and rollback
---

# Immutable release approval and rollback

Sailr supports two deployment contracts. Existing `workflow run` profiles keep the in-process
transactional deployment gate. Portable releases add an offline prepare phase and apply only the
canonical bytes stored in a `sailr.deployment-bundle/v1` file. Legacy `deploy` and `go` behavior is
unchanged.

## Portable release profile

```toml
[workflow.production]
environment = "production"
mode = "deploy"
engine = "runkernel"
interactive = false
build = "disabled"
push = "disabled"
generate = "run"
deploy = "run"
deploy_context = "production-cluster"
namespace = "app"
approval = "signature"
apply = true

[workflow.production.signature]
trusted_public_key = "<base64-encoded raw 32-byte Ed25519 public key>"

[workflow.production.verification]
rollout_timeout_seconds = 300

[workflow.production.rollback]
timeout_seconds = 300
```

The trusted public key is repository policy. Sailr never reads a replacement key from the
runtime environment and reports only its `sha256:<hex>` fingerprint. The private key must remain
outside Sailr.

The target environment may strengthen approval and configure release locking:

```toml
[deployment_policy]
required_approval = "signature"

[deployment_policy.release_lock]
lease_duration_seconds = 60
renew_interval_seconds = 20
# lease_name = "optional-fixed-name"
# lease_namespace = "optional-lock-namespace"
```

Environment names carry no implicit security meaning.

## Publication, promotion, and preparation

Every image-bearing workload for a build-backed service must use `{{service_image}}`. Normal
generation resolves it to the tagged service image. Portable preparation replaces it in memory
with the reviewed digest reference and verifies that every promoted image is present in the exact
bundled workload payload. Services without a `build` configuration are external dependencies and
retain explicit vendor image repositories with `{{service_version}}` in their templates.
Omitting `{{service_version}}` for an external workload emits a warning but does not fail
initialization or preparation.

```bash
sailr publication validate artifacts/publication-report.json
sailr promote plan \
  --from-report artifacts/publication-report.json \
  --to production \
  --out artifacts/promotion-plan.json
sailr workflow prepare production \
  --promotion-plan artifacts/promotion-plan.json \
  --out artifacts/prepared-release
```

Preparation is offline with respect to Docker, registries, Git, hooks, and Kubernetes. It creates
`deployment.bundle`, `deployment-plan.json`, `deployment.diff`, and
`preparation-evidence.json` in a new output directory. It rejects pre-deployment hooks. Exact
post-deployment hook commands are included in the bundle; database migrations should remain
separate CI stages.

## Signing and applying

Sign the UTF-8 bytes below with the configured Ed25519 private key:

```text
sailr-deployment-plan-v1:<64-character-plan-hash>
```

Then provide only the base64-encoded raw 64-byte signature:

```bash
export DEPLOY_APPROVAL_SIG="<base64 signature>"
sailr workflow apply production \
  --bundle artifacts/prepared-release/deployment.bundle \
  --non-interactive \
  --apply \
  --release-id "$CI_RELEASE_ID"
```

Apply revalidates the bundle, current profile, environment approval policy, target, service
coverage, target registry/repository policy, and signer fingerprint before Kubernetes access. It
never opens templates or regenerates manifests.

After approval, Sailr acquires a namespaced Kubernetes Lease. The default name is derived from
the environment, context, and namespace. The Lease is renewed through apply, rollout checks,
post-deployment hooks, and rollback; loss of ownership stops forward mutation. Process death is
recovered through Lease expiry without changing application resources.

Deployments, StatefulSets, and DaemonSets are polled for typed readiness every two seconds. Apply,
rollout, or post-hook failure triggers reverse-journal rollback within the configured timeout.
The resulting `sailr.workflow-report/v1` records approval, bundle and provenance digests, lock,
rollout, deployment, and rollback evidence.
