# Workflow Profiles (`sailr.workflow.toml`)

`sailr.workflow.toml` defines reusable CI/CD pipeline profiles executed via `sailr workflow run <profile>`.

## Example Profile
```toml
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

## Signed Deployment (Production)
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

Sailr renders only the key's `sha256:<hex>` fingerprint. The first run writes `.sailr/audit/<profile>/deployment-plan.json` and reports `awaiting_signature` without cluster mutation. Sign the exact ASCII message `sailr-deployment-plan-v1:<plan_hash>` outside Sailr, set only `DEPLOY_APPROVAL_SIG` to the base64 raw 64-byte signature, and retry. 
