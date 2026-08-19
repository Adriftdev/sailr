# Signed Deployment

Production deployments require an Ed25519 signature over the deployment plan hash. Set `DEPLOY_APPROVAL_SIG` in the environment.

Sign the exact ASCII message `sailr-deployment-plan-v1:<plan-hash>` after manual approval.
The trusted public key comes only from `sailr.workflow.toml`; never place the private key in
Sailr configuration or generated assets.
