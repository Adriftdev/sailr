# Delivery Flow Model

A delivery flow connects a source code change to a running environment.

## Key Concepts
- **Trigger**: What starts the flow (e.g., merge to main, manual approval, schedule).
- **Publication**: Building and pushing an immutable artifact to a registry.
- **Release**: Deploying a published artifact to an environment.

The separation of publication and release is fundamental. Images are built once and deployed multiple times.

Declare release orchestration under `[flow.<name>]` with candidate and optional signing
repository scripts plus ordered prepare, manual approval, optional sign, and apply stages.
Generate CircleCI with `sailr flow generate-ci [FLOW] --mode print|fragment|create|merge`.
The generated workflow is disabled by default and is selected by a Sailr-owned boolean pipeline
parameter. Configure the external CircleCI schedule with the emitted name, cron, branch, and set
that parameter to `true`; Sailr does not install schedules.
