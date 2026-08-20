# Delivery Flow Model

A delivery flow connects a source code change to a running environment.

## Key Concepts
- **Trigger**: What starts the flow (e.g., merge to main, manual approval, schedule).
- **Publication**: Building and pushing an immutable artifact to a registry.
- **Release**: Deploying a published artifact to an environment.

The separation of publication and release is fundamental. Images are built once and deployed multiple times.

Declare separate publication and release orchestration under `[flow.<name>]`. Publication uses a
branch trigger, publish stage, and typed storage adapter. Release uses a schedule/manual trigger,
digest-bound candidate manifest, prepare, manual approval, optional sign, and apply stages.
Every flow declares a checksummed Sailr release or full Git revision.
Generate CircleCI with `sailr flow generate-ci [FLOW] --mode print|fragment|create|merge`.
The generated workflow is disabled by default and is selected by a Sailr-owned boolean pipeline
parameter. Configure the external CircleCI schedule with the emitted name, cron, branch, and set
that parameter to `true`; Sailr does not install schedules.
