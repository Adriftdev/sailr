# Output Contract: DeliveryFlowPlan

The flow designer must produce a plan containing:

1. **Target Environments**: List of environments to configure.
2. **Deployment Model**: `GitOps` or `Direct Push` for each environment.
3. **CI Provider**: The CI system to use (e.g., CircleCI).
4. **Trigger Strategy**: How flows are initiated (e.g., merge to main, schedule).
5. **Approval Strategy**: Manual gate, automatic, or cryptographic signature.
6. **Required Credentials**: A list of required secrets (names and scopes only, NO VALUES).
