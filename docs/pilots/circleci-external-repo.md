# CircleCI External Repository Pilot

This document records the validation of Sailr operating within an external repository (Stage 3.5). 

## Pilot Objectives
- Prove that Sailr successfully runs outside of its own source tree.
- Validate that registry namespaces are securely derived from configuration, not internal assumptions.
- Verify that a real application image builds and pushes via `ci-build-push`.
- Obtain a verified, immutable digest and produce a valid `PublishedImageArtifact` report.
- Verify CI approval gating within CircleCI pipelines.

## Execution Record

- **Sailr CLI Revision:** [Insert Commit Hash]
- **External Repository Revision:** [Insert App Commit Hash]
- **CI Provider:** CircleCI
- **Environment:** `staging`
- **Registry Target:** [e.g. `ghcr.io/org/app`]

### Diagnostic Output
*To obtain the diagnostic configuration:*
```bash
sailr workflow inspect ci-build-push
```

### Required Commands
During the pipeline, the following commands should successfully run:

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

- **Published Image Ref:** `[Captured digest ref]`
- **Report Path:** `[Output path to the publication report]`
- **Issues Discovered:**
  - *Document any path resolution issues encountered during the external build.*
- **Fixes Applied:**
  - *Document configuration tweaks or Sailr patches required to unblock execution.*
