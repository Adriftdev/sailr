# CircleCI External Repository Pilot

This document records the validation of Sailr operating within an external repository (Stage 3.5). 

## Pilot Objectives
- Prove that Sailr successfully runs outside of its own source tree.
- Validate that registry namespaces are securely derived from configuration, not internal assumptions.
- Verify that a real application image builds and pushes via `ci-build-push`.
- Obtain a verified, immutable digest and produce a valid `PublishedImageArtifact` report.
- Verify CI approval gating within CircleCI pipelines.

## Execution Record

- **Sailr CLI Revision:** `4a9b2c8`
- **External Repository Revision:** `f21d3e4`
- **CI Provider:** CircleCI
- **Environment:** `staging`
- **Registry Target:** `ghcr.io/adriftdev/demo-app`

### Diagnostic Output
*To obtain the diagnostic configuration:*
```bash
sailr workflow inspect ci-build-push
```

### Required Commands
During the pipeline, the following commands successfully ran:

1. **Plan Phase:**
```bash
sailr workflow plan ci-build-push-plan
```

2. **Graph Rendering:**
```bash
sailr workflow graph ci-build-push-plan --format mermaid
```

3. **Dry-Run Plan Execution:**
```bash
sailr workflow run ci-build-push-plan --non-interactive
```

4. **Protected Publication:** *(requires external approval gate in CircleCI prior to execution)*
```bash
sailr workflow run ci-build-push --non-interactive --apply
```

## Results & Findings

- **Published Image Ref:** `ghcr.io/adriftdev/demo-app:staging-f21d3e4@sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- **Report Path:** `.sailr/reports/ci-build-push/latest.json`
- **Issues Discovered:**
  - Build paths defaulting to "." caused `checksums` to traverse the entire `.git` tree and local artifacts, leading to slow build cache hashing.
- **Fixes Applied:**
  - The `BuildOptions` in runner tests were updated to properly isolate the `cache_dir` in `tempfile` directories so tests don't pollute or fail on local environments.
  - Image reference generation was centralized to ensure tags are always properly qualified before they hit the registry.
  - The `WorkflowReport` generic envelope was implemented to reliably output JSON schemas for the CLI.
