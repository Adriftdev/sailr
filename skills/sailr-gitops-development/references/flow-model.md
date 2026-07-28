# Delivery Flow Model

A delivery flow connects a source code change to a running environment.

## Key Concepts

- **Trigger**: What starts the flow (e.g., merge to develop, manual approval, schedule).
- **Publication**: Building and pushing an immutable artifact to a registry.
- **Release**: Deploying a published artifact to an environment.

The separation of publication and release is fundamental. Images are built once and deployed multiple times.
