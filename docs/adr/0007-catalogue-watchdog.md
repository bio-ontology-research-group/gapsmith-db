# ADR 0007: Per-pathway watchdog in `propose-catalogue`

- **Status**: accepted
- **Date**: 2026-04-18
- **Phase**: 5 (follow-up)

## Context

The `propose-catalogue` batch driver walks a TSV of pathway names and
calls the OpenRouter-backed proposer once per row. Over the full
743-pathway gpt-4o run we observed two silent hangs — the Rust process
in `R` state, 90%+ CPU, **zero** open sockets, **zero** child
processes, **zero** syscalls over a 10 s strace window. Pure userspace
spin in the main thread, reproducible after ~10 minutes (~30
proposals) of healthy work, on random pathway names. Root cause
unidentified — likely a reqwest/tokio-blocking interaction, but we
could not attach `gdb` during the incident and `py-spy` refuses to
profile Rust.

The bounded retry in `post_with_retry` caps the OpenRouter *request*
cycle at ~9 min wall-time; the hang was *outside* that, somewhere
between bounded operations. So the catalogue driver itself was the
only place with enough context to enforce a ceiling.

Requirements:

1. A single stuck call must not consume the whole run.
2. Progress already written to `proposals/pending/` must be preserved
   (content-hashed filenames + `--resume` make this automatic if the
   loop keeps going).
3. Implementation cost low — a 743-row run is not worth a major
   runtime redesign.

## Decision

Wrap `proposer.propose(target)` in a worker thread with a bounded
`mpsc::channel` and `recv_timeout(120 s)`. On timeout we **detach**
the worker — the stuck thread keeps spinning until process exit, but
the catalogue loop records a `timeout` row in the per-run TSV and
moves to the next pathway.

Supporting changes:

- `OpenRouterBackend`, `Retrieval`, `PromptTemplate`, and
  `ProposerOptions` moved behind `Arc<>` in the driver so the worker
  thread gets `'static` references without cloning the whole
  structure per call.
- Every successful OpenRouter content is mirrored to
  `/tmp/openrouter_last.json` (overwritten per call). If the spinner
  recurs we now have the triggering payload on disk.
- Timeout and `no_reference`/`online_lookup_failed` warnings surface
  in the router as routing *reasons*, not as hard rejects.

## Consequences

- Worst-case wall-time per pathway is 120 s + throttle; a 743-row run
  is bounded to ~26 h even if every single call hangs (it doesn't).
- Leaked threads keep burning one CPU each until process exit. With a
  120 s timeout and a ~10-min mean-time-to-hang observed in the
  wild, a full run would typically leak 0–3 threads. Acceptable.
- `--resume` remains the recovery path: the catalogue keeps scanning
  for new hits rather than trying to recover an in-flight call.
- Next time we see a hang we can diff `/tmp/openrouter_last.json`
  against a healthy call and finally fix the root cause.

## Alternatives considered

- **`cargo install timeout` at the shell layer**. Too coarse: kills
  the whole process, losing every pathway in flight or in the
  in-process cache. Rules out `--resume` preserving the TSV.
- **Panic handler / `catch_unwind`**. Doesn't help — the spinner
  isn't panicking, it's running.
- **`std::thread::scope` with join timeout**. Rust stdlib doesn't
  expose a timed join; emulating it still requires the channel +
  detach pattern, so the `scope` ergonomics buy nothing.
- **`tokio::time::timeout` around an async proposer**. Requires
  threading the async context through the blocking reqwest client,
  which is exactly the stack we suspect causes the hang. Higher
  rewrite cost for no clear isolation win.
- **Install gdb, root-cause the spin, fix the bug**. Still on the
  table. The watchdog is defence-in-depth; it does not preclude a
  real fix, and `/tmp/openrouter_last.json` gives the next incident
  a fighting chance of being diagnosed.
