---
name: sailr-gitops-development
description: "Implement GitOps development flows using CircleCI and Sailr. Use this skill to configure PR checks, build-on-merge, and automated write-backs to Argo CD or Flux repositories. Do not use for production flows requiring explicit approval gates or scheduled releases (use sailr-circleci-production)."
---

# Sailr GitOps Development Flow

This skill generates and maintains GitOps delivery flows using CircleCI and Sailr write-backs.

## 1. Trigger

Activate this skill when the user asks to:

- "Set up GitOps for development"
- "Deploy via Argo CD on merge"
- "Configure PR preview environments"

## 2. Target Flow

1. **PR Check**: PR opened -> plan-only verification.
2. **Merge**: Code merged to develop.
3. **Publication**: Build and push immutable images.
4. **Write-back**: Update Git repository with new image digests.
5. **Reconcile**: Argo CD pulls changes and deploys.

## 3. GitOps Invariants

- CI must not possess cluster credentials.
- Deployments happen via Git write-back.
- Images must be identified by digest.

## 4. Interaction Contract

Present the plan, generate assets, and summarize required CI credentials (e.g., `GITHUB_TOKEN` for write-backs).
