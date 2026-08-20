# Build Model & Engines

Sailr supports two build engines:

## Global Build Policy (`[build]`)
```toml
[build]
engine = "runkernel"      # "roomservice" (default) or "runkernel"
fail_fast = false         # Roomservice policy; runkernel settles active siblings
max_parallelism = 4       # Concurrency level
before_all = "echo 'Starting builds'"
after_all = "echo 'Builds completed'"
```

`ignoreCache` is accepted as an alias for `ignore_cache`. Runkernel resolves and sorts exact input files; ignored patterns are relative to the build path. 

`--force` bypasses cache reads and writes for executable translated service tasks without deleting the prior cache. 

Global hooks and aggregates are never cached, and global hooks are suppressed when no selected service is dirty. Service `finally` commands run exactly once after active siblings settle, in stable reverse dependency order.

Sailr build outcome records are stored in `.sailr/cache/build`, and runkernel task-cache metadata is stored in `.sailr/cache/runkernel`. Sailr does not create a top-level `.runkernel` directory.
