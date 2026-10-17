# Controlled software CPU profiles

These are qualitative hotspot observations from a separate symbolized, controlled-I/O profile. They are not ABBA timing results or proof of speed changes. Raw perf.data, addresses and machine identity remain private.

Both captures passed integrity/control/isolation checks; reservations were removed and original CPU availability restored. Event loss and throttle counts, leaf resolution limits, positive stack mentions and source-line samples are reported in hotspots.json. An unresolved caller tail is not a complete trustworthy call graph.

The baseline/current captures contain 7,495/7,437 samples, with no lost or throttled records. Unresolved leaves account for 18.13%/18.92%; 87.14%/87.60% of samples contain at least one unresolved frame. Consequently these data cannot supply complete caller attribution or a per-function speed comparison. Resolved metadata/source observations informed the bounded-view and single-pass experiments; their actual throughput effect requires separate formal measurements.
