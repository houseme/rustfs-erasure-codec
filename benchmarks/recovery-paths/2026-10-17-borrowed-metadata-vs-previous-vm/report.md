# Borrowed recovery metadata streaming decode ABBA

Consumer: `902c97ce055b1faddb8cd689f0d3a04a0a3e6f35`.

Codec baseline: `9fcae450af8d98c5735e5c84bb42f28f9f018df8`. Candidate: `0a2d5ee74c62a83059cea857fb6645477d433020`.

All phases use the same plaintext oracle and observed sink. Each measured object is 4 MiB with 4+2 shards and 64 KiB blocks. Healthy and missing-first-data cases are measured with bitrot verification on and off; verification-off is a diagnostic control.

All-phase attribution allowed: true. Drift rejection count: 0 of four cells.

Reservation removed: True; parent CPUs restored: True.

This independent downstream profile does not replace the rejected 58-case codec campaign. Both sides use the same explicit GET/locality I/O defaults. The candidate combines in-place scoped planning with borrowed serial row and destination metadata; the main-baseline comparison also includes previously accumulated changes. The previous9f comparison isolates these combined runtime changes under the same benchmark. Rayon is configured for four workers; its actual pool count is unobserved because the consumer has no direct Rayon dependency. Actual Tokio workers and resolved backend selection are recorded in every phase. Allocation counts are not measured.

| Cell | Baseline drift | Candidate drift |
| --- | ---: | ---: |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-false` | 1.37% | 1.09% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-false_missing-first-data` | 0.37% | 0.19% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-true` | 0.22% | 0.13% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-true_missing-first-data` | 0.31% | 0.04% |

| Cell | Candidate throughput change |
| --- | ---: |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-false` | +0.46% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-false_missing-first-data` | +1.64% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-true` | +0.58% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-true_missing-first-data` | +1.14% |
