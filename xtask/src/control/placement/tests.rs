use super::*;
fn ready() -> Value {
    json!({"epoch":0,"cpu":2,"tid":501,"budget_before":2,"budget_after":2,"hypothetical_parallel_policy":{"use_parallel":true,"jobs":2,"chunk_len":32768}})
}
fn end() -> Value {
    json!({"epoch":0,"cpu":2,"cached_policy_unchanged":true,"available_parallelism_while_pinned":1})
}
#[test]
fn only_a_complete_owned_epoch_produces_a_gate_without_private_thread_ids() {
    let mut state = Handshake::default();
    state.ready(&ready(), &[2], &[2, 3], true, &[2, 3]).unwrap();
    let gate = state.end(&end(), &[2], 12, true, &[2]).unwrap();
    assert!(gate.get("tid").is_none());
    assert_eq!(gate["actual_affinity"], json!([2]));
    assert_eq!(state.ended, 1);
}
#[test]
fn bad_budgets_epochs_foreign_threads_and_premature_pinning_are_refused() {
    for (field, value) in [
        ("epoch", json!(1)),
        ("cpu", json!(3)),
        ("tid", json!(0)),
        ("budget_before", json!(1)),
        ("budget_after", json!(2.0)),
    ] {
        let mut value_event = ready();
        value_event[field] = value;
        assert!(
            Handshake::default()
                .ready(&value_event, &[2], &[2, 3], true, &[2, 3])
                .is_err(),
            "{field}"
        );
    }
    assert!(
        Handshake::default()
            .ready(&ready(), &[2], &[2, 3], false, &[2, 3])
            .is_err()
    );
    assert!(
        Handshake::default()
            .ready(&ready(), &[2], &[2, 3], true, &[2])
            .is_err()
    );
}
#[test]
fn end_requires_all_samples_a_stable_policy_and_the_exact_owned_affinity() {
    for (samples, owned, affinity) in [
        (11, true, vec![2]),
        (12, false, vec![2]),
        (12, true, vec![2, 3]),
    ] {
        let mut state = Handshake::default();
        state.ready(&ready(), &[2], &[2, 3], true, &[2, 3]).unwrap();
        assert!(state.end(&end(), &[2], samples, owned, &affinity).is_err());
    }
    let mut state = Handshake::default();
    state.ready(&ready(), &[2], &[2, 3], true, &[2, 3]).unwrap();
    let mut changed = end();
    changed["cached_policy_unchanged"] = false.into();
    assert!(state.end(&changed, &[2], 12, true, &[2]).is_err());
}
#[test]
fn repeated_epochs_retain_the_measuring_thread_and_previous_pin() {
    let mut state = Handshake::default();
    state
        .ready(&ready(), &[2, 3], &[2, 3], true, &[2, 3])
        .unwrap();
    state.end(&end(), &[2, 3], 12, true, &[2]).unwrap();
    let mut next = ready();
    next["epoch"] = 1.into();
    next["cpu"] = 3.into();
    next["tid"] = 502.into();
    assert!(state.ready(&next, &[2, 3], &[2, 3], true, &[2]).is_err());
    next["tid"] = 501.into();
    state.ready(&next, &[2, 3], &[2, 3], true, &[2]).unwrap();
}
