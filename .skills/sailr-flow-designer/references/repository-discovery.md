# Repository Discovery

To understand a repository's current state:

1. **Environments**: List `k8s/environments/`. Each subdirectory is an environment. Read `config.toml` to understand its purpose (e.g., `domain`, `extends`).
2. **Workflows**: Read `sailr.workflow.toml` to see existing profiles.
3. **CI Config**: Look for `.circleci/config.yml` or `.github/workflows/`.
4. **GitOps**: Look for `argocd/` or `flux/` directories containing deployment manifests.
