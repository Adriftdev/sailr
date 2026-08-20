---
name: sailr-circleci-production
description: "Implement production deployment flows using CircleCI and Sailr. Use this skill when the user wants to set up production deployments, scheduled releases, immutable image publication, or signed cryptographic deployment approvals. Do not use for development environments or GitOps write-backs (use sailr-gitops-development)."
---

# Sailr CircleCI Production Flow

This skill generates and maintains production delivery flows using CircleCI and Sailr direct push.

Start by running `sailr capabilities --format json`. Require `publication_consumption`,
`publication_execution`,
`multi_report_promotion`, `portable_deployment_bundle`, `publication_flow_generation`,
`release_flow_generation`, `circleci_generation`, `rollout_verification`, and `locking`.
If any capability is absent, stop and explain the installed version mismatch.
Never store credentials or private keys in repository files, and stop before production apply.

## 1. Trigger
Activate this skill when the user asks to:
- "Set up a production release"
- "Deploy to production from CircleCI"
- "Configure signed deployments"

## 2. Target Flow
The standard production flow is:
1. **Publication**: Commits to `main` run `sailr publication run <profile> --apply`, which
   builds and pushes immutable images and validates the generated report.
2. **Release Schedule**: A cron schedule triggers the release workflow.
3. **Candidate Selection**: Use an adapter to select reports and write a digest-bound manifest.
4. **Plan**: Validate the publication, create a promotion plan, and run `workflow prepare`.
5. **Approval**: Manual or signature-based gate.
6. **Deploy**: Run `workflow apply` against the persisted bundle without regeneration.
7. **Verify**: Sailr verifies workload rollout while holding the release Lease.

## 3. Production Invariants
- Deployments must never rebuild images.
- Generated jobs must use a checksummed Sailr release or a full Git revision.
- Images must be identified by digest (`sha256:...`).
- Deployments require an approval gate.
- Candidate reports must cover every build-backed target service, whose workload templates must
  use `{{service_image}}`. Services without `build` are external dependencies and retain their
  vendor image reference with `{{service_version}}`.
- Never store signing keys or bypass the approval boundary; stop before apply unless explicitly authorized.

## 4. CircleCI Requirements
- Use CircleCI contexts for credential injection.
- Ensure workflows have a clear `requires` graph.
- Use CircleCI artifacts as one storage adapter only when retention covers the release cadence;
  otherwise configure a repository storage script for GCS, S3, OCI, or an internal store.

Use `assets/sailr/production-flows.toml` as the flow-schema starting point, replacing every
placeholder and validating repository adapter paths before generation.

## 5. Interaction Contract
Do not generate files silently and finish. Present the plan, generate the assets, and then provide a summary of the credentials the user must configure in CircleCI to make the flow work.
