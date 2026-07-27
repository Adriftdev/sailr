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
<<<<<<< HEAD
build = "disabled"
=======
>>>>>>> d531c3a31777e14ad2f74934c8cdcea62d96d242
generate = "run"
deploy = "run"
deploy_context = "production-cluster"
namespace = "app"
approval = "signature"
apply = true
report = "both"
<<<<<<< HEAD

[workflow.production.signature]
trusted_public_key = "<base64-encoded raw 32-byte Ed25519 public key>"
```

The trusted key is configuration, not runtime input. Sailr validates it during
planning and displays only its `sha256:<hex>` fingerprint. Put the matching
root environment policy in `k8s/environments/production/config.toml`:

```toml
[deployment_policy]
required_approval = "signature"
```

Policy is explicit: environment names have no security meaning. The other
policy levels are `none` and `external`. The runner still requires the normal
`--apply` acknowledgement:
=======
```

The runner still requires the normal `--apply` acknowledgement:
>>>>>>> d531c3a31777e14ad2f74934c8cdcea62d96d242

```bash
sailr workflow run production --non-interactive --apply
```

The first unsigned run generates manifests, writes
`.sailr/audit/production/deployment-plan.json`, and stops at
`workflow:verification-gate`. The error displays the plan hash without mutating the cluster.

## Signing contract

<<<<<<< HEAD
The immutable bundle task reads all generated YAML once. It sorts paths,
preserves document order within each file, validates Kubernetes type/name
metadata and effective namespaces, rejects empty bundles and duplicate resource
identities, then retains canonical JSON bytes for every resource. Planning,
verification, application, rollback, and reporting share that in-memory bundle;
they do not reopen the YAML source files.

The audit artifact contains `payload` and `plan_hash`. The canonical payload
contains:

- schema identifier `sailr.audit/v1`;
- workflow profile and environment;
- target Kubernetes context and namespace;
- ordered `{relative_path, document_index, sha256}` records, where each digest
  covers the canonical JSON bytes that Sailr will apply.
=======
The audit payload contains:

- schema identifier `sailr.audit/v1`;
- workflow profile and environment;
- Kubernetes context and namespace;
- sorted manifest paths and SHA-256 digests.
>>>>>>> d531c3a31777e14ad2f74934c8cdcea62d96d242

Sailr computes the SHA-256 digest of the canonical JSON payload. Sign the UTF-8 bytes of:

```text
sailr-deployment-plan-v1:<64-character-plan-hash>
```

<<<<<<< HEAD
Provide only the base64-encoded raw 64-byte Ed25519 signature when retrying:

```bash
=======
Provide the base64-encoded raw Ed25519 values when retrying:

```bash
export DEPLOY_APPROVAL_PUBKEY="<base64 32-byte public key>"
>>>>>>> d531c3a31777e14ad2f74934c8cdcea62d96d242
export DEPLOY_APPROVAL_SIG="<base64 64-byte signature>"
sailr workflow run production --non-interactive --apply
```

The private key remains outside Sailr. On retry, deterministic build and generation tasks may
<<<<<<< HEAD
return `[CACHE]`; the bundle and verification gate are never cached. Sailr constructs a fresh
bundle on every run, so changing a manifest, profile, environment, context, or namespace changes
the plan hash and invalidates the signature. A missing signature is reported as
`awaiting_signature`; malformed or incorrect signatures are `failed`; a valid signature is
recorded as `verified` before any cluster mutation.

Text plans use stable `[CACHE]`, `[RUN]`, and `[SKIP]` markers. Use
`sailr workflow plan production --format json` for machine-readable tasks,
phase IDs, effects, cache policy, target, signer fingerprint, and finalizers.

## Rollback behavior

Sailr starts with an empty mutation journal. For each ordered bundle resource it reads the prior
object, applies the canonical bundle bytes, and journals the mutation only after a successful
apply. On the first apply failure it stops forward application and unwinds successful entries in
reverse order: updated objects are restored and newly created objects are deleted. A delete that
returns 404 is already restored. All rollback errors are retained and make the workflow fail.

A successfully completed deployment retains its journal so a later post-deployment task failure
can trigger the same idempotent reverse-order restoration. Runkernel waits for already-running
siblings to settle before Sailr runs service `finally` cleanup, in reverse service dependency
order. Cleanup runs exactly once even after failure; build cache records are written only after
the pipeline and cleanup both succeed; report persistence runs last.

Rollback covers Kubernetes objects managed by the generated manifests. Pre- and post-deployment
hooks may affect external systems and are observable but not automatically reversible.
=======
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
>>>>>>> d531c3a31777e14ad2f74934c8cdcea62d96d242
