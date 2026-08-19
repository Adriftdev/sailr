# Release Candidates

A release candidate is an immutable snapshot of published artifacts, selected for deployment.

The selection adapter writes `sailr.release-candidates/v1`. Each entry contains a safe path,
relative to the manifest, and the expected canonical `sha256:` publication-report digest.
Sailr reopens and validates each report and rejects replacements before creating
`sailr.promotion-plan/v1`. Use repeatable `--from-report` for direct selection or
`--from-manifest` for adapter output; equivalent selections produce the same plan.
