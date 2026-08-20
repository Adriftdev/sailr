# Security Invariants

All delivery flows must satisfy these safety rules:
1. **Immutable Artifacts**: Deployments must reference immutable image digests, never mutable tags (e.g., `latest`).
2. **No Rebuilds**: Deployments must not rebuild images. They must select existing published artifacts.
3. **Approval Gates**: Production deployments require explicit approval (manual or cryptographic signature).
4. **Credential Isolation**: Development flows must not have access to production credentials.
5. **Rollback Purity**: Rollbacks must only restore successful mutations.
6. **Bound Selection**: Candidate manifests bind canonical publication-report digests.
7. **Pinned Toolchain**: Release binaries have an expected SHA-256 or are built from a full commit.
8. **Scoped Consent**: Publication push uses CLI `--apply`; Kubernetes also requires profile `apply=true`.
