# Performance benchmark plans and shared fixtures

**The benchmarks themselves live with the crate they measure**, not here:

| Bench | Measures | ADR |
| --- | --- | --- |
| `cargo bench -p bp-buffer --bench large_file` | Opening and streaming a document too large to hold | [ADR-0027](../docs/decisions/ADR-0027.md) |
| `cargo bench -p bp-editor --bench scroll` | What one frame of scrolling costs, and whether it costs more further down | [ADR-0017](../docs/decisions/ADR-0017.md) |

Raw runs go under `artifacts/benchmarks/`, which is checked in -- a
measurement nobody kept is a measurement nobody can compare against.

**No criterion**, in either. It is not in the workspace dependency set, and
its model -- many iterations of a small operation, tracked over time -- is the
wrong shape for both of these: one is a one-shot with a page cache in it, and
the other asks whether a cost moves with the document, which wants two columns
of a single run compared rather than one number tracked across runs.

## What is still not measured, and it is deliberate

**Rasterisation cost and end-to-end input latency.** Both are the other half
of ADR-0017's revert condition, both need a capture rig this environment does
not have, and neither can be faked from inside the process. The scroll bench
says so in its own output rather than letting a green run imply more than it
checked.
