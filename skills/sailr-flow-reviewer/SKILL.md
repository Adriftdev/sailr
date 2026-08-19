---
name: sailr-flow-reviewer
description: "Review Sailr delivery flows for security, reliability, and correctness. Analyzes CircleCI config, GitOps setup, and Sailr workflow profiles against invariant rules. Use this skill when asked to review a deployment pipeline, check CI configuration, or audit a release process."
---

# Sailr Flow Reviewer

This skill audits delivery flows against Sailr invariants.

Query `sailr capabilities --format json` first and review the repository against the installed
contract rather than documentation guesses.
Refuse unsupported features, never request or store credentials/private keys, and stop before
any production apply.

## 1. Trigger
Activate this skill when the user asks to:
- "Review this delivery flow"
- "Audit our CI/CD setup"
- "Check if this pipeline is safe"

## 2. Review Scope
A full review covers:
- `sailr.workflow.toml` profiles.
- CI configuration (e.g., `.circleci/config.yml`).
- GitOps configuration (if applicable).
- Artifact building and pushing logic.

## 3. Core Principles
- Production flows require decoupled publication and release.
- Images must be identified by digest.
- Production deployments require explicit approval gates.
- Direct push requires credential isolation.
- GitOps write-backs must not trigger infinite loops.
- Direct production flows must use publication validation, full digest promotion, portable prepare/apply, rollout verification, and release locking.
- Do not infer production from names; use declared flow stages and environment policy.

## 4. Output Contract
Produce a structured report categorizing findings by severity (Blocker, High, Medium, Low, Advisory).
See `references/severity-model.md`.
