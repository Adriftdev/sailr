# General Review Rules

- All environments must have a defined deployment strategy.
- `sailr build` must not be used in the same job as `sailr deploy` for production.
- Block mutable/unversioned Sailr installers and release jobs that rebuild application images.
- Block candidate manifests whose paths are not paired with canonical report digests.
- Require complete selected-service coverage, a manual approval, matching signature stage/policy,
  and `serial-group` for direct production release application.
