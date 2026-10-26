//! Plan lifecycle diagnostic. Use --test for functional validation without a
//! statistical timing run; see docs/reconstruct-plan-benchmark-2026-10-26.md.

#[path = "reconstruct_plans/workload.rs"]
mod workload;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rustfs_erasure_codec::{Field, galois_8, galois_16};
use std::hint::black_box;
use workload::{ACTIONS, Fixture, SHARD_ELEMENTS, cases};

fn field_cases<F: Field>(criterion: &mut Criterion, field: &str) {
    let mut group = criterion.benchmark_group("reconstruct_plans");
    for case in cases::cases() {
        let id = format!(
            "{field}/{}x{}/{}/elements{SHARD_ELEMENTS}",
            case.data, case.parity, case.name
        );
        // One owner per case, outside Criterion's repeatedly invoked callback.
        // Validation also warms the inverse before every action's registration.
        let mut fixture = Fixture::<F>::new(case);
        for action in ACTIONS {
            fixture.validate();
            group.bench_function(BenchmarkId::new(action, &id), |bencher| {
                match action {
                    // iter consumes each Result and includes destruction of
                    // each returned owned plan in its measured lifecycle.
                    "prepare_full" => {
                        bencher.iter(|| black_box(black_box(&fixture).prepare_full()))
                    }
                    "prepare_none" => {
                        bencher.iter(|| black_box(black_box(&fixture).prepare_none()))
                    }
                    "call_none" => bencher.iter(|| black_box(black_box(&mut fixture).call_none())),
                    _ => unreachable!("fixed action catalog"),
                }
            });
            fixture.validate();
        }
    }
    group.finish();
}

fn plan_lifecycles(criterion: &mut Criterion) {
    field_cases::<galois_8::Field>(criterion, "gf8");
    field_cases::<galois_16::Field>(criterion, "tower-gf16");
}

criterion_group!(benches, plan_lifecycles);
criterion_main!(benches);
