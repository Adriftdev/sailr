---
title: Deterministic deployment audit gate
---

# Deterministic deployment audit gate

Sailr can require an Ed25519 signature over the exact generated manifests and deployment target
before it mutates a Kubernetes cluster. This is an opt-in, push-based gate; Sailr does not run a
background reconciler.

## Enable signature approval

```toml
[workflow.production]
environment = "production"
mode = "deploy"
generate = "run"
deploy = "run"
deploy_context = "production-cluster"
namespace = "app"
approval = "signature"
apply = true
report = "both"
```

The runner still requires the normal `--apply` acknowledgement:

```bash
sailr workflow run production --non-interactive --apply
```

The first unsigned run generates manifests, writes
`.sailr/audit/production/deployment-plan.json`, and stops at
`workflow:verification-gate`. The error displays the plan hash without mutating the cluster.

## Signing contract

The audit payload contains:

- schema identifier `sailr.audit/v1`;
- workflow profile and environment;
- Kubernetes context and namespace;
- sorted manifest paths and SHA-256 digests.

Sailr computes the SHA-256 digest of the canonical JSON payload. Sign the UTF-8 bytes of:

```text
sailr-deployment-plan-v1:<64-character-plan-hash>
```

Provide the base64-encoded raw Ed25519 values when retrying:

```bash
export DEPLOY_APPROVAL_PUBKEY="<base64 32-byte public key>"
export DEPLOY_APPROVAL_SIG="<base64 64-byte signature>"
sailr workflow run production --non-interactive --apply
```

The private key remains outside Sailr. On retry, deterministic build and generation tasks may
return `[CACHE]`; the verification gate itself is never cached. Sailr recomputes the artifact at
the gate, so changing a manifest, context, or namespace invalidates the signature.

## Rollback behavior

Before applying a workflow deployment, Sailr snapshots every managed target object. A partial
failure restores prior objects and deletes newly created objects in reverse order. A successfully
completed deployment also retains this journal for runkernel reverse-order rollback if a later
workflow task fails.

Rollback covers Kubernetes objects managed by the generated manifests. Pre- and post-deployment
hooks may affect external systems and are not automatically reversible. Rollback failures are
included in the workflow result and cause the run to fail.
