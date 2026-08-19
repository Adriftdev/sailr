# Release Candidates

A release candidate is an immutable snapshot of published artifacts, selected for deployment.

Validate it with `sailr publication validate`, then create `sailr.promotion-plan/v1` with
`sailr promote plan`. The report must be successful, digest-bearing, and cover every target
service. Sailr does not copy images between registries.
