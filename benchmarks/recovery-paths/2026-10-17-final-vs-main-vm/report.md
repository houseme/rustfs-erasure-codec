# Final runtime versus merged-main streaming decode ABBA

Consumer: `902c97ce055b1faddb8cd689f0d3a04a0a3e6f35`.

Codec baseline: `8f49bd8b174fa776309219c55c697b5b7ad9f05c`. Candidate: `9fcae450af8d98c5735e5c84bb42f28f9f018df8`.

All phases use the same plaintext oracle and observed sink. Each measured object is 4 MiB with 4+2 shards and 64 KiB blocks. Healthy and missing-first-data cases are measured with bitrot verification on and off; verification-off is a diagnostic control.

All-phase attribution allowed: true. Drift rejection count: 0 of four cells.

Reservation removed: True; parent CPUs restored: True.

This independent downstream profile does not replace the rejected 58-case codec campaign. Both sides use the same explicit GET/locality I/O defaults. The candidate combines single-pass planning and bounded native metadata; differences are not isolated to either change alone. Rayon is configured for four workers; its actual pool count is unobserved because the consumer has no direct Rayon dependency. Actual Tokio workers and resolved backend selection are recorded in every phase. Allocation counts are not measured.

| Cell | Baseline drift | Candidate drift |
| --- | ---: | ---: |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-false` | 0.88% | 0.34% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-false_missing-first-data` | 0.24% | 1.11% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-true` | 0.00% | 0.12% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-true_missing-first-data` | 0.03% | 0.20% |

| Cell | Candidate throughput change |
| --- | ---: |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-false` | +0.10% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-false_missing-first-data` | -2.39% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-true` | +0.07% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-true_missing-first-data` | -1.50% |

This comparison includes all accumulated runtime changes after `8f49bd8`, including earlier `c68c257` work. Only the separate `c68c257` comparison isolates the final two runtime corrections.
