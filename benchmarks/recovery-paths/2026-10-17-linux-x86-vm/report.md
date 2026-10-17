# Linux x86_64 recovery ABBA validation

Baseline: `8f49bd8b174fa776309219c55c697b5b7ad9f05c`. Candidate: `c68c2575ea11d1eb4c085b21e55f6b95da5975c3`.

Each of A1/B1/B2/A2 produced 58 complete case records (232 total). Each case used three 300 ms samples and four Rayon workers in an eight-vCPU exclusive VM cpuset partition. Physical-host exclusivity was not available.

Drift gate: 51/58 cases passed both 5% bookend limits. Whole-campaign latency attribution allowed: false.

The reservation was removed, parent CPUs were restored, and supplemental isolation checks passed.

Allocation attribution allowed: false. Allocation bookends are checked separately from latency.

No candidate speedup or regression is inferred. Candidate comparison ratios are null. All raw samples are retained; no failing row was rerun.

| Case | Baseline drift | Candidate drift |
| --- | ---: | ---: |
| `classic.construct/32x16/0/missing0+0/block0` | 5.01% | 0.48% |
| `classic.reconstruct_data_opt/10x4/1048576/missing2+0/block0` | 5.10% | 0.58% |
| `classic.reconstruct_opt_all_present/10x4/65/missing0+0/block0` | 17.00% | 1.67% |
| `classic.reconstruct_opt_all_present/40x8/65/missing0+0/block0` | 6.24% | 3.49% |
| `encode.serial/10x4/1048576/missing0+0/block0` | 17.87% | 2.52% |
| `encode.serial/8x3/1048576/missing0+0/block0` | 18.08% | 1.41% |
| `encode.serial/8x4/1048576/missing0+0/block0` | 19.28% | 1.28% |

See [manifest.json](manifest.json) for build/resource provenance, [summary.json](summary.json) for all 58 drift decisions, and [A1](A1.jsonl), [B1](B1.jsonl), [B2](B2.jsonl), [A2](A2.jsonl) for unchanged raw observations.
