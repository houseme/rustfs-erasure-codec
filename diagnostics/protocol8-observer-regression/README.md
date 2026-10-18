# Native observer allocation regression and launch refusal

The original protocol-7 observer fails the actual Linux zero-allocation assertion:
16 observations allocate 32 times, reallocate 80 times and deallocate 32 times.
The corrected protocol-8 observer passes with all three counters zero. The separate
counter is not linked into formal capture. The corrected shared sampler (eight
active tests), observer executable (six tests) and Slot tracer (two tests) also
pass on native Linux. Tool source is 4722b753abcd46a347be058624fd2bc2ffe97c6b;
original observer source is bound in qualification.json.

The first full-control launcher was rejected before any performance observations
because its ancestor coordinator was still in the CPU partition. This refusal
and successful resource restoration are retained, not silently overwritten.
A fresh direct-controller full campaign uses the same frozen tool and unchanged
isolation validation. Its outcome will be preserved separately. This qualification
cannot approve performance or release; management/build logs stay private.
