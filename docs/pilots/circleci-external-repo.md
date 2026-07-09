# CircleCI External Repo Pilot

## External repo
https://github.com/josh-tracey/sailr-travis-pilot

## Sailr revision
`experiment/runkernel`

## CI provider
CircleCI

## Registry
`europe-west2-docker.pkg.dev` (GCP Artifact Registry)

## Service
`skyfleet`

## Profiles tested
- `ci-build-push-plan`
- `ci-build-push`

## What passed
- Sailr installed from Git revision.
- Push plan ran in CircleCI.
- CircleCI approval gate configured and worked.
- Sailr accepted `approval = external`.
- Image pushed to registry (GCP).
- Digest captured.
- Report validated with jq.

## Issues found
- Initial failure: `CI push requires approval=external`.
- Resolution: Kept `approval = external` in `sailr.workflow.toml` and added `approve_image_push` job of `type: approval` to CircleCI configuration to align manual approval gates with Sailr's CI safety constraints.

## Follow-up work
- Provider-specific error messages implemented in Sailr (`src/workflow/runner.rs`) to guide developers towards correct CI setups for CircleCI, GitHub Actions, and Travis.
