# Validation Checklist

Before presenting a delivery flow plan, verify:
- [ ] All environments map to exactly one deployment model (Direct or GitOps).
- [ ] No CI provider conflicts (e.g., mixing CircleCI and GitHub Actions for the same flow).
- [ ] Artifact publication is decoupled from deployment (especially for production).
- [ ] Production flows require approval.
