# ADR 0001: Sailr Delivery Boundaries

**Date:** 2026-07-11
**Status:** Accepted

## Context

As Sailr evolves from a local development orchestration tool to a continuous deployment platform, there is a temptation to fold specific deployment technologies (like Argo CD, Flux, or direct Kubernetes applies) directly into Sailr's core abstractions. 

This coupling leads to:
1. Hard dependencies on external tool APIs.
2. Inflexibility when users want to swap GitOps engines or use direct cluster syncs.
3. Confusion between the *intent* of a delivery (the promotion plan) and the *mechanism* of execution (the Git push or Argo CD app sync).

## Decision

**Sailr owns app-delivery intent and workflow evidence.** 

Deployment targets apply or reconcile desired state.

- **GitOps is a deployment strategy, not a core workflow primitive.** 
- **Argo CD is an optional GitOps provider**, not the central ontology.
- **Sailr does not become a long-running cluster reconciler.**

Sailr's responsibilities conclude when a generic desired-state diff is securely produced or, optionally, applied/pushed to a neutral repository boundary. Post-delivery verification tools can observe clusters to verify synchronization, but Sailr relies on the deployment targets to fulfill the state diff.

## Consequences

1. Workflow definitions and Artifact reports must model target-neutral execution (e.g. `DeliveryTargetKind` -> `GitOps`, `KubernetesDirect`).
2. Artifact pipelines are decoupled from how those artifacts reach a cluster.
3. Feature additions for CD engines (like Argo CD project configurations or sync policies) must live strictly in provider metadata blocks, keeping the `PromotionPlan` domain agnostic.
