use super::*;
use std::sync::atomic::AtomicBool;
#[cfg(unix)]
#[test]
fn a_validation_error_does_not_restart_or_desynchronize_the_frozen_worker() {
    let cancelled = AtomicBool::new(false);
    let context = Context::new(&cancelled);
    let mut command = Command::new("sh");
    command.args(["-c",r#"read -r line; printf '%s\n' '{"ok":false,"error":"refused"}'; read -r line; printf '%s\n' '{"ok":true,"value":18446744073709551615}'; read -r line"#]);
    let mut session = Session::spawn(&context, command).unwrap();
    let pid = session.process.child.id();
    assert!(
        session
            .request(json!({"request":1}))
            .unwrap_err()
            .to_string()
            .contains("refused")
    );
    assert_eq!(session.request(json!({"request":2})).unwrap(), u64::MAX);
    assert_eq!(session.process.child.id(), pid);
    drop(session);
    assert!(!context.unreaped.get());
}
#[cfg(unix)]
#[test]
fn cancellation_stops_only_the_owned_frozen_session() {
    let cancelled = AtomicBool::new(true);
    let context = Context::new(&cancelled);
    let mut command = Command::new("sh");
    command.args(["-c", "sleep 30"]);
    let mut session = Session::spawn(&context, command).unwrap();
    assert!(
        session
            .request(json!({}))
            .unwrap_err()
            .to_string()
            .contains("cancelled")
    );
    drop(session);
    assert!(!context.unreaped.get());
}
#[cfg(unix)]
#[test]
fn malformed_duplicate_reply_fields_cannot_become_success() {
    let cancelled = AtomicBool::new(false);
    let context = Context::new(&cancelled);
    let mut command = Command::new("sh");
    command.args([
        "-c",
        r#"read -r line; printf '%s\n' '{"ok":false,"ok":true,"value":1}'"#,
    ]);
    let mut session = Session::spawn(&context, command).unwrap();
    assert!(session.request(json!({})).is_err());
}
