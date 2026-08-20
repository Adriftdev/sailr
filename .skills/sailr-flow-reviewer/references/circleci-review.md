# CircleCI Review Rules

- Ensure approval jobs exist before deploy jobs.
- Check context usage for secrets.
- Verify branch filters on workflows.
- Verify prepared artifacts cross the approval boundary through a workspace without regeneration.
- Verify the apply job uses `serial-group` and Sailr still enforces its Kubernetes Lease.
- Scheduled trigger installation remains external to generated configuration.
