# Downstream streaming decode ABBA

Consumer: `902c97ce055b1faddb8cd689f0d3a04a0a3e6f35`.

Codec baseline: `8f49bd8b174fa776309219c55c697b5b7ad9f05c`. Candidate: `c68c2575ea11d1eb4c085b21e55f6b95da5975c3`.

All phases use the same plaintext oracle and observed sink. Each measured object is 4 MiB with 4+2 shards and 64 KiB blocks. Healthy and missing-first-data cases are measured with bitrot verification on and off; verification-off is a diagnostic control.

All-phase attribution allowed: true. Drift rejection count: 0 of four cells.

Reservation removed: True; parent CPUs restored: True.

This independent downstream profile does not replace the rejected 58-case codec campaign. Rayon is configured for four workers; its actual pool count is unobserved because the consumer has no direct Rayon dependency. Actual Tokio workers and resolved backend selection are recorded in every phase. Allocation counts are not measured.

| Cell | Baseline drift | Candidate drift |
| --- | ---: | ---: |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-false` | 1.54% | 1.17% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-false_missing-first-data` | 1.55% | 0.54% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-true` | 0.38% | 0.43% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-true_missing-first-data` | 0.40% | 0.41% |

| Cell | Candidate throughput change |
| --- | ---: |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-false` | +0.07% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-false_missing-first-data` | -2.62% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-true` | -0.15% |
| `streaming_decode/decode/4+2_4096KB_64KB-block_verify-true_missing-first-data` | -1.47% |

The four phases used copies of the same immutable parent environment, but individual RustFS GET/locality I/O tuning values were not recorded. This is a reproducibility boundary: defaults cannot be assumed retroactively, and later explicit-I/O campaigns must remain separate. The recorded degraded-read signal is retained; it is not overall #2853 acceptance.
