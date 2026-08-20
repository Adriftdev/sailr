# Sailr Workflow Contract

Workflow profiles (`sailr.workflow.toml`) map delivery stages to Sailr commands:

- **Check**: Run tests and linting (`mode = "check"`). No cluster mutation.
- **Build**: Build and publish images (`mode = "build"`).
- **Deploy**: Update the cluster (`mode = "deploy"`).
- **Go**: End-to-end build and deploy (typically for local dev, `mode = "go"`).
