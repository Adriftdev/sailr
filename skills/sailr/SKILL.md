---
name: sailr
description: "Understand, configure, and operate Sailr projects. Use for Sailr environment files, service definitions, build configuration, Kubernetes templates, workflow profiles, CLI commands, configuration linting, migration, manifest generation, and core Sailr troubleshooting. Do not use as the primary skill for designing complete CircleCI production releases or Argo CD development flows; use the specialised Sailr delivery-flow skills for those tasks."
---

# Sailr Core Operating Procedures

This skill is the core control plane for understanding, configuring, and operating Sailr projects.

## 1. Task Identification

When you receive a Sailr-related task, first determine if the task is better suited for a specialised delivery-flow skill.

If the task involves:
- Designing or reviewing end-to-end delivery flows: **Use `sailr-flow-designer` or `sailr-flow-reviewer`**.
- Configuring CircleCI production publication and deployment: **Use `sailr-circleci-production`**.
- Configuring PR checks, merge-publication, or GitOps write-back: **Use `sailr-gitops-development`**.

Otherwise, use this core `sailr` skill to modify environments, templates, workflows, or run Sailr CLI commands.

## 2. Repository Inspection

When starting work on a Sailr repository, inspect the layout:
- `k8s/environments/` contains `config.toml` environment definitions.
- `k8s/templates/` contains Kubernetes manifest templates.
- `sailr.workflow.toml` in the repository root defines CI/CD workflow profiles.

## 3. Reference Loading

Load the detailed reference documents from `references/` into your context using `view_file` when you need specific schema details:

- **`references/environment-schema.md`**: For modifying `config.toml` files, configuring services, variables, or environment inheritance.
- **`references/template-variables.md`**: For injecting Sailr template placeholders (`{{ var }}`) into Kubernetes manifests.
- **`references/workflow-profile-schema.md`**: For modifying `sailr.workflow.toml` workflow profiles, stages, approval policies, or signatures.
- **`references/cli-reference.md`**: For running `sailr` CLI commands, syntax, options, and commands.
- **`references/build-model.md`**: For build caching behaviour, engine differences (runkernel vs roomservice), and phase hooks.
- **`references/deployment-safety.md`**: For understanding rollback semantics, plan-only purity, and signed deployment invariants.

## 4. Execution

- Execute modifications carefully. Ensure TOML syntax is correct.
- If generating CI or delivery flows using legacy methods, warn the user that they should be using the specialised delivery-flow skills.
- Use `sailr lint` to validate configuration changes.
- Use `sailr workflow plan` to verify workflow profiles.
- When running builds locally for testing, use `sailr build --name <env>`.

## 5. Result Reporting

- Summarise changes clearly.
- If you ran commands that mutated state (e.g. `sailr infra up` or `sailr deploy`), report the outcome.
- For workflow plans, ensure the plan executes as expected and highlight any unexpected mutations or cache misses.
