# Security Invariants

All delivery flows must satisfy these safety rules:
1. **Immutable Artifacts**: Deployments must reference immutable image digests, never mutable tags (e.g., `latest`).
2. **No Rebuilds**: Deployments must not rebuild images. They must select existing published artifacts.
3. **Approval Gates**: Production deployments require explicit approval (manual or cryptographic signature).
4. **Credential Isolation**: Development flows must not have access to production credentials.
5. **Rollback Purity**: Rollbacks must only restore successful mutations.
