use super::*;
use alloc::sync::Arc;
use parking_lot::{Condvar, Mutex};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Barrier,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Default)]
struct BuildGate {
    open: Mutex<bool>,
    changed: Condvar,
}

impl BuildGate {
    fn wait(&self) {
        let mut open = self.open.lock();
        while !*open {
            self.changed.wait(&mut open);
        }
    }

    fn release(&self) {
        *self.open.lock() = true;
        self.changed.notify_all();
    }
}

fn key(data: usize, missing: usize) -> DecodePlanKey {
    let mut present = SmallVec::from_elem(true, data + 4);
    present[missing] = false;
    (data, 4, present)
}

fn build(key: &DecodePlanKey) -> LeopardGf16DecodePlan {
    let driver = build_leopard_gf16_decode_driver(key.0, key.1, 64).unwrap();
    let missing_data: Vec<_> = (0..key.0).filter(|&i| !key.2[i]).collect();
    let missing_parity: Vec<_> = (key.0..key.0 + key.1).filter(|&i| !key.2[i]).collect();
    build_decode_plan(
        &driver,
        &key.2,
        &missing_parity,
        &missing_data,
        init_leopard_gf16_tables(),
    )
}

fn await_pending_callers(cache: &Mutex<DecodePlanCache>, callers: usize) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Some(cache) = cache.try_lock()
            && cache.pending.len() == 1
            && cache.pending[0].value.strong_count() == callers
        {
            return true;
        }
        std::thread::yield_now();
    }
    false
}

fn cold_wave(
    cache: &Mutex<DecodePlanCache>,
    key: &DecodePlanKey,
    panic_first: bool,
) -> (usize, usize, Vec<Arc<LeopardGf16DecodePlan>>) {
    const CALLERS: usize = 8;
    let start = Barrier::new(CALLERS + 1);
    let gate = BuildGate::default();
    let builds = AtomicUsize::new(0);
    let (joined, results) = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..CALLERS)
            .map(|_| {
                scope.spawn(|| {
                    start.wait();
                    catch_unwind(AssertUnwindSafe(|| {
                        with_cached_decode_plan(cache, key.clone(), || {
                            let attempt = builds.fetch_add(1, Ordering::Relaxed);
                            gate.wait();
                            assert!(
                                !panic_first || attempt != 0,
                                "injected plan initializer panic"
                            );
                            build(key)
                        })
                    }))
                })
            })
            .collect();
        start.wait();
        let joined = await_pending_callers(cache, CALLERS);
        // Release workers before checking assertions so failures can join them.
        gate.release();
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        (joined, results)
    });
    assert!(
        joined,
        "every cold caller must join the registered plan cell"
    );
    let failures = results.iter().filter(|result| result.is_err()).count();
    let plans: Vec<_> = results.into_iter().filter_map(Result::ok).collect();
    assert!(plans.iter().all(|plan| Arc::ptr_eq(plan, &plans[0])));
    assert!(cache.lock().pending.is_empty());
    (builds.load(Ordering::Relaxed), failures, plans)
}

#[test]
fn same_key_cold_wave_builds_once_and_warm_hits_skip_coordination() {
    let cache = Mutex::new(DecodePlanCache::new(8 * 1024 * 1024));
    let key = key(300, 0);
    let expected = build(&key);
    let (builds, failures, plans) = cold_wave(&cache, &key, false);
    assert_eq!(builds, 1);
    assert_eq!(failures, 0);
    assert_eq!(plans[0].err_locs, expected.err_locs);
    assert_eq!(plans[0].input_count, expected.input_count);
    assert_eq!(cache.lock().entries.len(), 1);
    let retained = cache.lock().retained_bytes;
    let warm = with_cached_decode_plan(&cache, key, || panic!("warm plan rebuilt"));
    assert!(Arc::ptr_eq(&warm, &plans[0]));
    assert_eq!(cache.lock().retained_bytes, retained);
    assert!(cache.lock().pending.is_empty());
    eprintln!("GF16_COLD_PLAN_COALESCED callers=8 builds={builds} cached=1");
}

#[test]
fn oversized_and_zero_budget_results_are_shared_for_the_current_wave() {
    for budget in [
        0,
        build(&key(10, 0)).retained_bytes() + core::mem::size_of::<DecodePlanKey>(),
    ] {
        let cache = Mutex::new(DecodePlanCache::new(budget));
        let small_key = key(10, 0);
        let small = with_cached_decode_plan(&cache, small_key.clone(), || build(&small_key));
        let before = cache.lock().retained_bytes;
        let large_key = key(300, 0);
        let (builds, failures, plans) = cold_wave(&cache, &large_key, false);
        assert_eq!(builds, 1);
        assert_eq!(failures, 0);
        assert!(cache.lock().get(&large_key).is_none());
        assert_eq!(cache.lock().retained_bytes, before);
        if budget != 0 {
            assert!(Arc::ptr_eq(&cache.lock().get(&small_key).unwrap(), &small));
        }
        let next = with_cached_decode_plan(&cache, large_key.clone(), || build(&large_key));
        assert!(!Arc::ptr_eq(&next, &plans[0]));
        assert_eq!(cache.lock().retained_bytes, before);
    }
}

#[test]
fn a_panicking_initializer_allows_waiters_to_retry_the_same_cell() {
    let cache = Mutex::new(DecodePlanCache::new(8 * 1024 * 1024));
    let (builds, failures, plans) = cold_wave(&cache, &key(300, 0), true);
    assert_eq!(builds, 2);
    assert_eq!(failures, 1);
    assert_eq!(plans.len(), 7);
    assert_eq!(cache.lock().entries.len(), 1);
}

#[test]
fn abandoned_panic_registrations_are_reclaimed_on_the_next_miss() {
    let cache = Mutex::new(DecodePlanCache::new(8 * 1024 * 1024));
    for missing in 0..32 {
        let failed = catch_unwind(AssertUnwindSafe(|| {
            with_cached_decode_plan(&cache, key(300, missing), || panic!("injected panic"))
        }));
        assert!(failed.is_err());
        let cache = cache.lock();
        assert_eq!(cache.pending.len(), 1);
        assert_eq!(cache.pending[0].value.strong_count(), 0);
    }
    let key = key(300, 0);
    with_cached_decode_plan(&cache, key.clone(), || build(&key));
    assert!(cache.lock().pending.is_empty());
    assert_eq!(cache.lock().entries.len(), 1);
}

#[test]
fn distinct_keys_build_concurrently_without_holding_the_cache_mutex() {
    let cache = Mutex::new(DecodePlanCache::new(8 * 1024 * 1024));
    let gate = BuildGate::default();
    let (ready, received) = mpsc::channel();
    let both_ready = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..2)
            .map(|missing| {
                let ready = ready.clone();
                let cache = &cache;
                let gate = &gate;
                scope.spawn(move || {
                    let key = key(300, missing);
                    with_cached_decode_plan(cache, key.clone(), || {
                        ready.send(()).unwrap();
                        gate.wait();
                        build(&key)
                    })
                })
            })
            .collect();
        let first = received.recv_timeout(Duration::from_secs(10)).is_ok();
        let second = received.recv_timeout(Duration::from_secs(10)).is_ok();
        gate.release();
        for handle in handles {
            handle.join().unwrap();
        }
        first && second
    });
    assert!(both_ready);
    assert_eq!(cache.lock().entries.len(), 2);
    assert!(cache.lock().pending.is_empty());
}

#[test]
fn registration_limits_allow_existing_waiters_and_independent_overflow_builds() {
    let cache = Mutex::new(DecodePlanCache::new(8 * 1024 * 1024));
    let owners: Vec<_> = (0..MAX_PENDING_DECODE_PLANS)
        .map(|missing| cache.lock().pending_for(&key(32, missing)).unwrap())
        .collect();
    assert_eq!(cache.lock().pending.len(), MAX_PENDING_DECODE_PLANS);
    assert!(Arc::ptr_eq(
        &cache.lock().pending_for(&key(32, 0)).unwrap(),
        &owners[0],
    ));
    let other = key(300, 0);
    assert!(cache.lock().pending_for(&other).is_none());
    let plan = with_cached_decode_plan(&cache, other.clone(), || build(&other));
    assert!(Arc::ptr_eq(&cache.lock().get(&other).unwrap(), &plan));
    assert_eq!(cache.lock().pending.len(), MAX_PENDING_DECODE_PLANS);
    drop(owners);
    let next = key(300, 1);
    with_cached_decode_plan(&cache, next.clone(), || build(&next));
    assert!(cache.lock().pending.is_empty());
}

#[test]
fn registration_key_bytes_are_bounded_and_reclaimed_after_owners_drop() {
    let mut cache = DecodePlanCache::new(8 * 1024 * 1024);
    let mut owners = Vec::new();
    let slots = MAX_PENDING_DECODE_KEY_BYTES / decode_key_bytes(&key(20000, 0).clone());
    assert!(slots > 0 && slots < MAX_PENDING_DECODE_PLANS);
    for pattern in 0..slots {
        let key = key(20000, pattern);
        owners.push(cache.pending_for(&key).unwrap());
    }
    let extra = key(20000, slots);
    assert!(cache.pending_for(&extra).is_none());
    let bytes: usize = cache
        .pending
        .iter()
        .map(|entry| decode_key_bytes(&entry.key))
        .sum();
    assert!(bytes <= MAX_PENDING_DECODE_KEY_BYTES);
    assert!(cache.pending.len() < MAX_PENDING_DECODE_PLANS);
    let oversized = (
        MAX_PENDING_DECODE_KEY_BYTES,
        1,
        SmallVec::from_elem(true, MAX_PENDING_DECODE_KEY_BYTES),
    );
    assert!(cache.pending_for(&oversized).is_none());
    drop(owners);
    let owner = cache.pending_for(&key(10, 0)).unwrap();
    assert_eq!(cache.pending.len(), 1);
    cache.finish_pending(&owner);
    assert!(cache.pending.is_empty());
}

#[test]
fn completion_of_an_older_wave_preserves_a_new_registration_for_the_same_key() {
    let mut cache = DecodePlanCache::new(0);
    let key = key(10, 0);
    let first = cache.pending_for(&key).unwrap();
    cache.finish_pending(&first);
    let second = cache.pending_for(&key).unwrap();
    cache.finish_pending(&first);
    assert_eq!(cache.pending.len(), 1);
    assert!(Arc::ptr_eq(&cache.pending_for(&key).unwrap(), &second));
    cache.finish_pending(&second);
    assert!(cache.pending.is_empty());
}

#[test]
fn borrowed_presence_hash_matches_owned_keys_across_inline_and_heap_boundaries() {
    use core::hash::BuildHasher;

    let hash_builder = hashlink::DefaultHashBuilder::default();
    for len in [14, 256, 257, 304, 512, 65536] {
        let mut present = SmallVec::from_elem(true, len);
        present[0] = false;
        present[len - 1] = false;
        let owned: DecodePlanKey = (len - 4, 4, present);
        assert_eq!(
            hash_builder.hash_one(&owned),
            hash_builder.hash_one((owned.0, owned.1, owned.2.as_slice())),
            "{len} shards"
        );
    }
}

#[test]
fn borrowed_lookup_checks_every_key_component_when_hashes_collide() {
    use core::hash::BuildHasher;

    let hash_builder = hashlink::DefaultHashBuilder::default();
    let mut cache = DecodePlanCache::with_hasher(8 * 1024 * 1024, hash_builder.clone());
    let key = key(300, 0);
    let plan = Arc::new(build(&key));
    let hash = hash_builder.hash_one(&key);
    cache.insert(key.clone(), plan.clone());
    let bytes = cache.retained_bytes;
    assert!(Arc::ptr_eq(
        &cache.get_hashed(hash, key.0, key.1, &key.2).unwrap(),
        &plan
    ));
    assert!(cache.get_hashed(hash, key.0 + 1, key.1, &key.2).is_none());
    assert!(cache.get_hashed(hash, key.0, key.1 + 1, &key.2).is_none());
    assert!(
        cache
            .get_hashed(hash, key.0, key.1, &key.2[..key.2.len() - 1])
            .is_none()
    );
    let mut other = key.2.clone();
    *other.last_mut().unwrap() = false;
    assert!(cache.get_hashed(hash, key.0, key.1, &other).is_none());
    assert_eq!(cache.retained_bytes, bytes);
}

#[test]
fn borrowed_hits_refresh_recency_before_byte_budget_eviction() {
    use core::hash::BuildHasher;

    let first_key = key(10, 0);
    let second_key = key(10, 1);
    let third_key = key(10, 2);
    let first = Arc::new(build(&first_key));
    let second = Arc::new(build(&second_key));
    let third = Arc::new(build(&third_key));
    let bytes = first.retained_bytes() + decode_key_bytes(&first_key);
    let hash_builder = hashlink::DefaultHashBuilder::default();
    let mut cache = DecodePlanCache::with_hasher(bytes * 2, hash_builder.clone());
    cache.insert(first_key.clone(), first.clone());
    cache.insert(second_key.clone(), second);
    let hash = hash_builder.hash_one(&first_key);
    assert!(Arc::ptr_eq(
        &cache
            .get_hashed(hash, first_key.0, first_key.1, &first_key.2)
            .unwrap(),
        &first
    ));
    cache.insert(third_key.clone(), third);
    assert!(cache.get(&second_key).is_none());
    assert!(Arc::ptr_eq(&cache.get(&first_key).unwrap(), &first));
    assert!(cache.get(&third_key).is_some());
    assert_eq!(cache.retained_bytes, bytes * 2);
}
