# Deployment Safety & Rollback Scope

## Deployment Policy
Cluster security is explicit and never inferred from an environment name:

```toml
[deployment_policy]
required_approval = "signature" # none | external | signature
```

## Rollback Scope
Sailr journals only successful Kubernetes mutations.
Partial failure restores updates and deletes creates in reverse order; later post-deploy failure can reuse the completed journal. 
Hook side effects are observable but not reversible. 
Rollback errors are aggregated and fail the workflow.

## Plan-Only Purity
Plan execution evaluates the dependency graph and computes cache eligibility but performs NO mutations.
Planning performs no registry mutation, no Git mutation, no cluster mutation, and executes no deployment hooks.
