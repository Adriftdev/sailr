# CLI Command Reference

## Project & Service Initialization
* **Initialize a new environment**: `sailr init --name <ENV_NAME> [--engine roomservice|runkernel]`
* **Add a new service**: `sailr add service <SERVICE_NAME> --type web-app|worker|database`

## Configuration Utilities
* **Lint configuration**: `sailr lint [--config <PATH>]`
* **Migrate configuration to schema 0.5.0**: `sailr migrate --name <ENV_NAME> [--engine roomservice|runkernel]`
* **Bump service version**: `sailr bump --service <SERVICE_NAME> --version <NEW_VERSION> [--env <ENV_NAME>]`

## Core Operations (Build, Generate, Deploy, Go)
* **Build container images**: `sailr build --name <ENV_NAME> [--force] [--engine roomservice|runkernel] [--ignore <SVC1,SVC2>]`
* **Generate Kubernetes manifests**: `sailr generate --name <ENV_NAME> [--only <SVC1,SVC2>] [--ignore <SVC3>]`
* **Deploy to Kubernetes context**: `sailr deploy --name <ENV_NAME> --context <K8S_CONTEXT> [--strategy Rolling|Restart]`
* **Combined All-in-One**: `sailr go --name <ENV_NAME> --context <K8S_CONTEXT> [--force] [--strategy Rolling|Restart]`

## Infrastructure & Interactive Commands
* **Spin up infrastructure**: `sailr infra up <ENV_NAME> --provider Local`
* **Tear down infrastructure**: `sailr infra down --name <ENV_NAME>`
* **Launch Interactive TUI mode**: `sailr interactive --context <K8S_CONTEXT> --name <ENV_NAME>`

## Workflow Management (`sailr workflow`)
* **List workflow profiles**: `sailr workflow list`
* **Show profile configuration**: `sailr workflow show <PROFILE>`
* **Plan a workflow run**: `sailr workflow plan <PROFILE> [--format text|json]`
* **Run a workflow profile**: `sailr workflow run <PROFILE> [--dry-run] [--apply]`
* **Export workflow dependency graph**: `sailr workflow graph <PROFILE> --format mermaid`
* **Generate CI configuration**: `sailr workflow generate-ci <PROFILE> --provider github`
