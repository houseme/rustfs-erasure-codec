# 9.1.0 publication

On November 13, 2026, the maintainer approved publishing 9.1.0 using completed
functional and compatibility acceptance, with performance acceptance tracked
separately. The selected workflow input is `publish_9_1_0_functional=true` on
the main branch.

The publication job depends on the complete ordinary CI matrix, including
security, generated WASM bindings, release tooling, platform builds and tests.
It checks the exact 9.1.0 manifest/workspace/lock versions, builds the actual
crate with a locked dry-run, and uploads through the repository's crates.io
publishing credential.

After upload, the job downloads the registry archive and verifies its checksum,
local package equality and Git source identity. It then creates the matching
`9.1.0` tag and GitHub Release. A retry checks any existing registry version
and tag against the tested source before completing publication.

Performance records remain available for continued investigation, including
the Classic and GF16 cache follow-ups and all retained drift observations.
[Classic cache follow-up](classic-cache-hashes-2026-11-13.md),
[GF16 cache follow-up](gf16-plan-cache-followup-2026-11-13.md)

## Run

```sh
gh workflow run ci.yml --repo houseme/rustfs-erasure-codec --ref main \
  -f publish_9_1_0_functional=true
```

The workflow's registry receipt and GitHub Release response are retained in
the `publication-9.1.0-*` Actions artifact.
