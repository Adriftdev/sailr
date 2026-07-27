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
- service hooks and commands are represented as deterministic phase tasks with
  IDs such as `service:api:run_parallel:0`; the aggregate remains
  `service:api:build`.
- service inputs are resolved and sorted before they enter runkernel. Explicit
  cache keys include command and configuration fingerprints.

## Try it

```bash
sailr build --name dev --engine runkernel --plan
sailr build --name dev --engine runkernel --explain
sailr build --name dev --engine runkernel
sailr init --name dev --engine runkernel
sailr migrate --name dev --engine runkernel
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

- Roomservice remains the default unless `--engine runkernel` or `[build].engine = "runkernel"` is selected.
- `sailr init` and `sailr migrate` preserve Roomservice unless
  `--engine runkernel` is explicit. Migration converts schema 0.5.0 and records
  the engine without switching existing configurations automatically.
- Deployment hooks can have external side effects and are not automatically reversible.
- runkernel cache metadata is stored under `.runkernel/cache`; Sailr build outcome records remain under `.sailr/cache/build`.

The runkernel translator exposes command phases as deterministic graph nodes such as
`service:api:run_parallel:0` and retains `service:api:build` as the service completion node.
Synchronous commands chain, parallel commands fan out from their preceding
dependency and join before the next phase, and dependent services point to the
dependency service aggregate. `max_parallelism` is enforced across executable
phase nodes. `--force` bypasses cache reads and writes without deleting prior
cache state.

Service `finally` commands are post-settlement Sailr finalizers rather than
forward graph tasks. Runkernel stops new scheduling after failure and waits for
active siblings; Sailr then runs dirty-service cleanup once in stable reverse
dependency order. Cache writing is success-only and report persistence is last.
