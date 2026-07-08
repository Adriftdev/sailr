---
title: Roomservice to runkernel
---

# Migrating from Roomservice to runkernel backend

## Status

The runkernel backend is experimental and opt-in.

Roomservice remains the default build backend. Use runkernel when you want to try the workflow-backed build engine while keeping a simple rollback path.

## What stays the same

- `sailr build`
- `sailr go`
- `--only`
- `--ignore`
- `--force`
- `--plan`
- `--dry-run`
- `--explain`
- `--dump-scope`
- service build config
- build hooks

## What changes

- runkernel backend stores cache under `.sailr/cache/build`.
- Roomservice stores cache under `.roomservice`.
- runkernel uses `SailrBuildPlan` plus a runkernel `Pipeline` internally.
- service builds are represented as service-level workflow tasks.

## Try it

```bash
sailr build --name dev --engine runkernel --plan
sailr build --name dev --engine runkernel --explain
sailr build --name dev --engine runkernel
```

You can also opt in through config:

```toml
[build]
engine = "runkernel"
fail_fast = false
```

## Roll back

```bash
sailr build --name dev --engine roomservice
```

Or remove `[build].engine = "runkernel"` from `config.toml`; the default backend is still Roomservice.

## Known limitations

- `[build].max_parallelism` is accepted but not enforced by the runkernel backend yet.
- runkernel currently uses service-level tasks, not phase-level graph nodes.
- `sailr workflow graph` and `sailr workflow explain` are planned for a later release.
