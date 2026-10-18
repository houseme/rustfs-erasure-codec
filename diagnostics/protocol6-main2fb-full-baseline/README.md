# Complete protocol-6 baseline prerequisite: main 2fb7f38

The tool is merged main `2fb7f38926d7c95395707cc060785a9f0e23f8a6`; the workload is published 9.0.0. This is a baseline-only prerequisite under the default allocator and formal basic telemetry, not candidate attribution or release evidence.

All 126 cases complete calibration and A1: 1,134 measured observations plus all warmups/probes. Four distributions fail the 10% width gate: smoke update 32+16/1MiB (28.4347%) and three stream-encode cases (46.3534%, 22.7809%, 35.4604%). Nine samples of ShardSlot recovery 4+2/64KiB fail the 50ms wall minimum. No recorded known-Vec layout mismatches occur. The predeclared quality stop prevents A2 and any candidate measurement or attribution.

At 4,096 iterations, the final ShardSlot calibration changes from a 49ms first warmup to roughly 428–445ms for later warmups/probes; its separate measurement worker stays around 48ms. Exported backend/features/seed/count agree. The measuring-thread runqueue wait is low; these observations do not establish a unique underlying allocator, layout, memory, kernel-path or host-scheduling cause. Increasing counts or weakening limits would not prove the two execution states comparable.

The derived analysis replays actual complete raw observations through merged main's timing/shape validators: small-file duration is rejected, and the four wide distributions remain visible. It never invents a complete candidate manifest or positive strict-preflight result.

All copied JSON/TSV files and the generic controller are byte-for-byte unchanged. The initialization/calibration/warmup/sample records, source/context bindings and restoration outcomes are retained. Ancillary build/console/resource-management files contain internal paths and remain private. No credentials, full virtual addresses or real user objects are present. The CPU partition and controller state were restored.

The later protocol-7 writer-reservation correction is a distinct shared workload. This rejected protocol-6 snapshot cannot qualify that changed source or policy. Fresh full calibration, paired evidence and positive strict preflight remain required.
