# In-place scoped planning streaming decode ABBA

Consumer: `902c97ce055b1faddb8cd689f0d3a04a0a3e6f35`.

Codec baseline: `9fcae450af8d98c5735e5c84bb42f28f9f018df8`. Candidate: `5d2406876ad79b6729d60f1f874457aab76724f8`.

All phases use the same plaintext oracle and observed sink. Each measured object is 4 MiB with 4+2 shards and 64 KiB blocks. Healthy and missing-first-data cases are measured with bitrot verification on and off; verification-off is a diagnostic control.

All-phase attribution allowed: true. Drift rejection count: 0 of four cells.

Reservation removed: True; parent CPUs restored: True.

This independent downstream profile does not replace the rejected 58-case codec campaign. Both sides use the same explicit GET/locality I/O defaults. The new runtime change constructs scoped Classic plans in place; the main-baseline comparison also includes previously accumulated changes. The previous9f comparison isolates this runtime change under the same benchmark. Rayon is configured for four workers; its actual pool count is unobserved because the consumer has no direct Rayon dependency. Actual Tokio workers and resolved backend selection are recorded in every phase. Allocation counts are not measured.

| Cell | Baseline drift | Candidate drift |
| --- | ---: | ---: |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-false` | 0.35% | 0.46% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-false_missing-first-data` | 0.78% | 0.33% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-true` | 0.15% | 0.24% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-true_missing-first-data` | 0.09% | 0.10% |

| Cell | Candidate throughput change |
| --- | ---: |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-false` | -0.58% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-false_missing-first-data` | +1.60% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-true` | -0.34% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-true_missing-first-data` | +0.56% |
