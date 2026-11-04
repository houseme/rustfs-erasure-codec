# Local encode/reconstruct/verify ABBA: published 9.0.0 vs path 9.1.0

Date: 2026-11-04  
Host: Apple M5 Max, `aarch64-apple-darwin`, rustc 1.99.1, release profile  
Backend: `rust-neon` on **both** versions (`simd-accel`, `RSE_BACKEND_OVERRIDE` unset)  
Threads: `RAYON_NUM_THREADS=4`, `RS_PARALLEL_POLICY_MAX_JOBS=4`  
Baseline: crates.io `rustfs-erasure-codec =9.0.0` (tag `9.0.0` → `43a9f15` / `9891dbe`)  
Candidate: this repository path `9.1.0` at archive time `8252010`

**This is a local screen, not performance acceptance or release approval.**
CPU resources were not reserved; drift refusals are retained rather than dropped.

## Files

| File | Purpose |
| --- | --- |
| `paired_encode_decode_probe.rs` | Measurement logic (A1/B1/B2/A2) |
| `Cargo.toml.example` | Isolated crate linking `base_codec`/`next_codec` |
| `full-33case.jsonl` | Expanded catalog run (33 cases, 3 drift refusals) |
| `full-33case.stderr.txt` | Drift refusal lines from that run |
| `run1-6case.jsonl` / `run3-6case.jsonl` | Earlier 6-shape confirmation runs |
| `run2-partial.jsonl` | Interrupted confirmation (drift gate) |
| `allocation-report.json` | 45-case request-count report (both versions) |

## Method

1. Build an isolated package outside the workspace with exact pins:
   - `base_codec = =9.0.0` (crates.io, `simd-accel`)
   - `next_codec = =9.1.0` path into this repository (`simd-accel`)
2. For each shape: assert byte-identical encode, bidirectional verify, and
   reconstruct of lost `data[0]` (+ `parity[0]` when parity ≥ 2) against encoded shards.
3. ABBA phases per operation: A1 (9.0.0) → B1 (9.1.0) → B2 (9.1.0) → A2 (9.0.0).
4. Each phase: 50 warmups, three samples of ≥300 ms in batches of 8.
5. Gate: relative drift between the two A (or B) medians > **8%** marks the case
   `drift_refused=true`. Refusals are printed and kept; they are not attributed.
   Exit code 3 if any refusal occurred.
6. Throughput GB/s uses payload `data_shards × shard_len` over the faster phase median.

Reproduce (from archive directory):

```bash
mkdir -p /tmp/rse-abba-full/src
cp Cargo.toml.example /tmp/rse-abba-full/Cargo.toml
# rewrite next_codec path to the absolute repository root in that Cargo.toml
cp paired_encode_decode_probe.rs /tmp/rse-abba-full/src/main.rs
cd /tmp/rse-abba-full
unset RSE_BACKEND_OVERRIDE
RAYON_NUM_THREADS=4 RS_PARALLEL_POLICY_MAX_JOBS=4 cargo run --release
# optional: RSE_ABBA_LOCAL_DRIFT=0.08
```

## Expanded run (33 cases) — 2026-11-04

Summary line from `full-33case.jsonl`:

```json
{"cases":33,"drift_refusals":3,"mean_encode_ratio":0.9676,"mean_reconstruct_ratio":0.9911,"mean_verify_ratio":0.8854,"max_drift_pct":12.85,"release_approval":false}
```

Drift refusals (not attributed): `4x2_64k`, `8x4_1m`, `12x4_64k`.

**Accepted (30 cases, drift ≤ 8%):**

| Metric | Mean Δ (candidate vs baseline) | Notes |
| --- | --- | --- |
| encode | **−3.6%** (faster) | 26/30 faster, 4/30 slower; range −19.6% … +1.3% |
| reconstruct | **−1.0%** (faster) | 18/30 faster, 12/30 slower; range −6.3% … +4.2% |
| verify | **−11.1%** (faster) | larger layout sensitivity; treat as indicative only |

Negative delta = 9.1.0 is faster.

Absolute throughput examples (9.1.0, same host): Classic 4+2/64 KiB encode ≈ 33 GB/s;
10+4/1 MiB encode ≈ 12–13 GB/s. See JSONL for per-case `base_GBps` / `next_GBps`.

## Earlier 6-shape confirmation

Two independent 6-shape runs (`run1`, `run3`) agreed on direction:

| Shape | encode Δ | reconstruct Δ |
| --- | --- | --- |
| 4+2/64 KiB | −0.4% / −0.6% | −0.5% / −1.0% |
| 10+4/1 MiB | −4.5% / −4.7% | ≈0% |
| 12+4/32 KiB | −9.3% / −8.4% | ≈−2% |
| 8+4/1 KiB | −3.3% / −3.4% | −7.5% / −7.5% |

## Allocation report

`allocation-report.json` (45 cases, release, both `rust-neon`):

- Classic encode: 0 requests both versions.
- Cold-inverse Classic recover: 6 → 2 requests.
- Leopard GF8 encode 7 → 0; recover 27 / ~1.0 MB → 2 / 64 KiB.
- Leopard GF16 encode 19 → 0; recover 40 / ~1.5 MB → 2 / 64 KiB.
- Stream encode/verify/reconstruct request counts drop sharply (e.g. 41→8, 40→10).

Request counts are not RSS, latency, or release evidence.

## Companion: `galois_backend` ABBA (repository script)

Also on 2026-11-04, the tracked script was run with full SHAs (not symbolic
`HEAD`, which becomes the baseline after the first switch) and `RSE_ABBA_BACKEND=auto`
so Criterion group names embed `override_auto` and pass provenance validation:

```bash
RSE_ABBA_BACKEND=auto RSE_ABBA_FEATURES='std simd-accel' \
RSE_ABBA_SAMPLE_SIZE=10 RSE_ABBA_WARMUP_TIME=1 RSE_ABBA_MEASUREMENT_TIME=1 \
RSE_ABBA_COOLDOWN_SECONDS=5 RSE_ABBA_DRIFT_THRESHOLD=0.05 \
RSE_ABBA_OUT_DIR=target/benchmark-smoke/galois-backend-abba-local-2026-11-04-v2 \
bash scripts/run_galois_backend_abba.sh 43a9f15dc432ddece3d535c426e30a44cee27633 \
  8252010eeedc97c74f4eea95ee749b8a5dda07c6
```

Archived under `galois-backend-abba/`:

- Input gate: **PASS**
- Drift gate: **PASS**
- Candidate attribution: **PERMITTED**
- Commits: A=`9891dbe` (9.0.0) / B=`8252010` (candidate)
- 6/6 rows PASS; B vs A mean ns delta **+0.13% … +1.68%** (candidate microbench
  slightly slower on these NEON GF mul lengths under a 1 s/10-sample budget —
  within gates, not a throughput claim for full encode/reconstruct).

An earlier invocation with `RSE_ABBA_BACKEND=rust-neon` and candidate ref `HEAD`
failed validation (A/B both resolved to `9891dbe`; XOR group names truncated so
`backend_override` conflicted with the embedded token). That run is **not** evidence.

## Formal 126-case campaign (outstanding)

Full ABBA/paired-layout acceptance still requires the controlled freeze/capture
path in `docs/release-9.1.0-blockers.md` (isolated resources, frozen protocol,
5% repeat / 10% central-width gates across all 126 cases). This archive does not
run or satisfy that campaign.

## Boundaries

- Functional compatibility is covered separately by
  `scripts/release-compatibility` (200 + 36 + 12 + 32 cases; see
  `docs/9.0.0-to-9.1.0-codec-delta.md`).
- Fast-one-parity is **disabled** in the compatibility probe by design.
- Formal 126-case ABBA / paired-layout acceptance and strict positive preflight
  remain outstanding (see `docs/release-9.1.0-blockers.md`); this directory does
  not close those gates.
- Do not quote a single refused case or the unfiltered mean as a release claim.
