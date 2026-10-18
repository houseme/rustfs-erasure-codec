# Complete protocol-10 baseline variability replay

This directory derives observations from the unchanged complete default and retained-arena baseline controls. It starts no workers and changes no measurements. Both measured codecs are published 9.0.0, using frozen measurement tool 63b6fa0. SOURCE.json identifies the later offline analyzer source; it is not the source of a new campaign.

Every input SHA256 matches the already public raw member byte-for-byte. The default report retains all 126 A1 cases (seven wide distributions); no A2 or repeat claim is invented. The retained report retains all 126 A1/A2 cases, all 3,024 warmup/sample trace observations, all 252 passing widths, and all seven repeat failures. No stable subset is promoted.

| Measured CPU endpoint grouping | All cases | Repeat failures over 5% |
| --- | ---: | ---: |
| One identical endpoint CPU in both phases | 16 | 2 |
| One endpoint CPU per phase, differing between phases | 100 | 4 |
| Multiple endpoint CPUs in a phase | 10 | 1 |

The first group includes ordinary encode 10+4/512 KiB on CPU 29 (15.827029% drift), and optimized recovery 4+2/64 KiB on CPU 31 (18.164593%). Maximum recorded main-thread runnable-wait/wall across those two cases is 0.019055%. Changing endpoint CPUs or recorded main-thread runqueue delay alone cannot explain all failures. Equal endpoints cannot exclude intermediate migration; these are not affinity-pinning proofs. Main-thread counters omit Rayon workers, and matching caller offsets do not establish physical-page/cache/internal-output equality. No unique CPU/allocator/host cause is assigned.

Replay with source commit 9b9b5537d390d01ad32555810aa157efe64c3015 checked out separately:

```sh
python3 source/scripts/analyze_baseline_variability.py --capture audit/diagnostics/protocol10-default-baseline --output new-default-analysis.json
python3 source/scripts/analyze_baseline_variability.py --capture audit/diagnostics/protocol10-retained-arena-baseline --output new-retained-analysis.json
```

The copied analyzer is an audit copy; run it with the matching source repository modules above. This supplements original frozen-validator/isolation verification rather than replacing it. Reports always set release_approval=false and candidate_attribution=null. Counter reads bracket the wall timer; run/wall ratios above one remain visible without clamping. Original raw data, failures and all earlier audit stages remain unchanged.
