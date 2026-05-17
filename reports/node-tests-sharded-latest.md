# Node tests latest (sharded)

Status: `pass`

Total files: 57
Selected/completed/passed: 57/57/57

This report is a shard aggregation because this execution environment kills long single-command runs. Each shard was executed with the same node test runner and non-overlapping `--shard i/5` selection.

| Shard | Status | Selected | Completed | Passed | Batches |
|---:|---|---:|---:|---:|---:|
| 1/5 | pass | 12 | 12 | 12 | 8/8 |
| 2/5 | pass | 12 | 12 | 12 | 5/5 |
| 3/5 | pass | 11 | 11 | 11 | 4/4 |
| 4/5 | pass | 11 | 11 | 11 | 5/5 |
| 5/5 | pass | 11 | 11 | 11 | 5/5 |

Single-command `npm run test:repo` remains suitable for normal CI, but was not used as the final evidence in this constrained tool runtime.
