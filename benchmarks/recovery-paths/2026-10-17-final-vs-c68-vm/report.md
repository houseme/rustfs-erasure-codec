# Final runtime versus previous-runtime streaming decode ABBA

Consumer: `902c97ce055b1faddb8cd689f0d3a04a0a3e6f35`.

Codec baseline: `c68c2575ea11d1eb4c085b21e55f6b95da5975c3`. Candidate: `9fcae450af8d98c5735e5c84bb42f28f9f018df8`.

All phases use the same plaintext oracle and observed sink. Each measured object is 4 MiB with 4+2 shards and 64 KiB blocks. Healthy and missing-first-data cases are measured with bitrot verification on and off; verification-off is a diagnostic control.

All-phase attribution allowed: true. Drift rejection count: 0 of four cells.

Reservation removed: True; parent CPUs restored: True.

This independent downstream profile does not replace the rejected 58-case codec campaign. Both sides use the same explicit GET/locality I/O defaults. The candidate combines single-pass planning and bounded native metadata; differences are not isolated to either change alone. Rayon is configured for four workers; its actual pool count is unobserved because the consumer has no direct Rayon dependency. Actual Tokio workers and resolved backend selection are recorded in every phase. Allocation counts are not measured.

| Cell | Baseline drift | Candidate drift |
| --- | ---: | ---: |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-false` | 0.59% | 0.76% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-false_missing-first-data` | 0.84% | 0.62% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-true` | 0.03% | 0.05% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-true_missing-first-data` | 0.17% | 0.08% |

| Cell | Candidate throughput change |
| --- | ---: |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-false` | +0.26% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-false_missing-first-data` | +0.35% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-true` | +0.07% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-true_missing-first-data` | +0.29% |

Every cell has overlapping phase median 95% confidence intervals. The positive point estimates do not demonstrate a speedup. Neither change is isolated from the other; no cross-campaign pooling is performed.
