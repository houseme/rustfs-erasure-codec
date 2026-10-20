# Local Docker ARM64 preflight and acceptance status, 2026-10-20

The local engine and Rust image execute ARM64 on an ARM64 host. The unchanged
`recovery_paths` executable at clean source
`a55e0e7bfacf67c8c103f2370dc82c108e649453` completed its entire catalog:
72 cases and 216 samples, without a case filter or timing retry. Each case keeps
20 original warmup iterations and three one-second samples, with four Rayon
threads and eight eligible logical CPUs. Source/builds live on the container's
Linux filesystem. The reported GF8 byte backend is NEON; that label does not
trace every internal kernel. All original parity, corruption and recovery checks
passed. Complete raw numeric observations and private provenance remain local;
this document publishes execution status only, not performance measurements.

## Isolation prevents formal release acceptance

The dedicated privileged container exposes cgroup v2. Its owned partition became
invalid with an empty effective exclusive mask; ancestor exclusive-mask writes
returned EPERM. All probe changes were restored exactly. The unchanged formal
collector refuses the real container with
`a valid exclusive cgroup CPU partition is required`.
A taskset/CPU eligibility mask is insufficient for the existing exclusive resource
contract. See the [kernel cpuset documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset-interface-files).
No hypervisor, host policy, governor or unrelated container was changed.

This is a current-source diagnostic, not the 126-case release profile. Its
three-sample/20-warmup schedule differs from protocol 15's nine samples/three
warmups. There is no formal baseline qualification, candidate ABBA, paired-layout
acceptance, accepted evidence or release approval. Raw values are not normalized
or used to relax any gate. #2897 retains its existing completion marks; #2853's
native/downstream acceptance is not inferred from a container diagnostic.

## Independent ARM provenance correction

The collector formerly required cpuinfo `model name`, which this ARM64 kernel
omits. Its actual implementer/architecture/variant/part/revision and ISA flags are
available. The correction records canonical guest-visible identities for every
reserved CPU and retains named-model strings unchanged. Incomplete, malformed
or duplicate CPU data are refused; no unknown physical CPU model is invented.
The actual container fields and native tests pass. Physical-host identity and
exclusivity remain unproven; the formal isolation refusal remains in place.
Worker code, protocol 15, algorithm compatibility and performance gates are
unchanged. Provenance support does not turn the diagnostic into release evidence.

For accepted capture, use an ARM64 Docker/Linux environment with a valid delegated
exclusive cpuset partition and the complete existing provenance interfaces. Then
complete dynamic-buffer/lifetime/layout acceptance, qualify the entire baseline,
and collect final candidate/layout ABBA under the retained drift, distribution,
duration and regression gates. Only accepted immutable evidence and actual strict
positive preflight permit evaluating the 9.1.0 tag and upload.
