---
name: sailr-flow-designer
description: "Design end-to-end delivery flows for Sailr repositories. Analyses repositories to recommend and generate CI/CD plans mapping code to production. Use this skill when asked to setup CI, design a deployment pipeline, figure out how to release, or generate a delivery plan. Do not use for implementing specific GitOps write-backs or CircleCI production gates (delegate to specialised skills)."
---

# Sailr Delivery-Flow Designer

This skill designs end-to-end delivery flows mapping source code to running environments.

## 1. Trigger
Activate this skill when the user asks to:
- "Set up CI/CD for this repository"
- "Generate a delivery flow"
- "Figure out how we should deploy this"
- "Create a deployment plan"

## 2. Repository Discovery
Before designing a flow, inspect the repository (see `references/repository-discovery.md`). Determine:
- Environments present (`k8s/environments/`)
- Existing CI configuration (`.circleci/`, `.github/`)
- Existing GitOps configuration (`argocd/`, `flux/`)

## 3. Decision Rules
- If the environment extends a production profile or uses signed deployments, recommend **CircleCI Production Flow**.
- If the environment is for development/preview and the team prefers GitOps, recommend **GitOps Development Flow**.
- Never mix direct push and GitOps in the same environment.

## 4. Output Contract
The skill must produce a `DeliveryFlowPlan` outlining the environments, CI providers, and deployment models chosen. See `references/output-contract.md`.

## 5. Implementation Delegation
Once the design is approved, do not implement it yourself. Delegate to:
- **`sailr-circleci-production`** for production flows.
- **`sailr-gitops-development`** for GitOps/development flows.

## 6. Ambiguity Flagging
Flag any ambiguity in environment purpose or CI provider choice before proceeding.
