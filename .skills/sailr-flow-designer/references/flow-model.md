# Delivery Flow Model

A delivery flow connects a source code change to a running environment.

## Key Concepts
- **Trigger**: What starts the flow (e.g., merge to main, manual approval, schedule).
- **Publication**: Building and pushing an immutable artifact to a registry.
- **Release**: Deploying a published artifact to an environment.

The separation of publication and release is fundamental. Images are built once and deployed multiple times.

Declare publication as `kind = "publication"` with a branch trigger and one publish stage.
Declare release as `kind = "release"` with a schedule/manual trigger and
prepare -> approval -> optional sign -> apply. Every flow declares a tagged toolchain using a
checksummed release binary or full Git revision. Release candidate adapters emit
`sailr.release-candidates/v1`, binding safe relative report paths to canonical report digests.
