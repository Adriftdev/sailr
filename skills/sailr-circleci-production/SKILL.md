---
name: sailr-circleci-production
description: "Implement production deployment flows using CircleCI and Sailr. Use this skill when the user wants to set up production deployments, scheduled releases, immutable image publication, or signed cryptographic deployment approvals. Do not use for development environments or GitOps write-backs (use sailr-gitops-development)."
---

# Sailr CircleCI Production Flow

This skill generates and maintains production delivery flows using CircleCI and Sailr direct push.

## 1. Trigger
Activate this skill when the user asks to:
- "Set up a production release"
- "Deploy to production from CircleCI"
- "Configure signed deployments"

## 2. Target Flow
The standard production flow is:
1. **Publication**: Commits to `main` build and push immutable images.
2. **Release Schedule**: A cron schedule triggers the release workflow.
3. **Candidate Selection**: Select the latest published artifacts.
4. **Plan**: Generate a plan-only deployment report.
5. **Approval**: Manual or signature-based gate.
6. **Deploy**: Push changes to the cluster.
7. **Verify**: Post-deployment verification.

## 3. Production Invariants
- Deployments must never rebuild images.
- Images must be identified by digest (`sha256:...`).
- Deployments require an approval gate.

## 4. CircleCI Requirements
- Use CircleCI contexts for credential injection.
- Ensure workflows have a clear `requires` graph.

## 5. Interaction Contract
Do not generate files silently and finish. Present the plan, generate the assets, and then provide a summary of the credentials the user must configure in CircleCI to make the flow work.
