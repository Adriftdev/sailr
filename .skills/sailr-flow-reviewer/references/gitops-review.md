# GitOps Review Rules

- Write-back jobs must push via PAT or deploy key.
- Check for infinite loop prevention (e.g., `[skip ci]` in commit msg or branch filters).
