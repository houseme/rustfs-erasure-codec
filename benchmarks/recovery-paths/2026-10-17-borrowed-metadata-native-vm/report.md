# Linux x86_64 recovery ABBA validation

Baseline: `8f49bd8b174fa776309219c55c697b5b7ad9f05c`. Candidate: `0a2d5ee74c62a83059cea857fb6645477d433020`.

All four phases produced {'A1': 66, 'B1': 66, 'B2': 66, 'A2': 66}. Each case used three 1000 ms samples and four Rayon workers in an eight-vCPU exclusive VM cpuset partition. Physical-host exclusivity was not available.

Drift gate: 59/66 cases passed both 5% bookend limits. Whole-campaign latency attribution allowed: false.

Reservation removed: True; parent CPUs restored: True; isolation passed: True.

Allocation attribution allowed: false. Allocation bookends are checked separately from latency.

No candidate speedup/regression conclusion is drawn. Candidate comparison ratios are null. All raw samples are retained; no failing row was rerun.

| Case | Baseline drift | Candidate drift |
| --- | ---: | ---: |
| `classic.reconstruct_data_opt/10x4/32768/missing2+0/block0` | 1.72% | 7.11% |
| `classic.reconstruct_parity.cached/10x4/1024/missing0+1/block0` | 8.04% | 4.08% |
| `classic.reconstruct_some_parity/32x16/65536/missing1+1/block0` | 4.23% | 9.57% |
| `encode.serial/4x2/1024/missing0+0/block0` | 3.76% | 6.47% |
| `encode.serial/4x2/1048576/missing0+0/block0` | 2.25% | 6.90% |
| `encode.serial/8x3/1048576/missing0+0/block0` | 0.88% | 6.95% |
| `encode.serial/8x4/1048576/missing0+0/block0` | 5.93% | 0.26% |
