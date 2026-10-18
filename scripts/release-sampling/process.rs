//! Diagnostic-only process observations, outside every benchmark timer.
use std::collections::BTreeMap;
use std::fs;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

fn fields(stat: &str) -> Vec<&str> {
    stat.rsplit_once(") ")
        .unwrap()
        .1
        .split_whitespace()
        .collect()
}

pub(super) fn snapshot() -> String {
    static SLOTS: OnceLock<Mutex<BTreeMap<(u64, u64), usize>>> = OnceLock::new();
    let start = Instant::now();
    let Ok(own_stat) = fs::read_to_string("/proc/thread-self/stat") else {
        return "{\"available\":false}".into();
    };
    let own_tid: u64 = own_stat.split_whitespace().next().unwrap().parse().unwrap();
    let process_stat = fs::read_to_string("/proc/self/stat").unwrap();
    let process_fields = fields(&process_stat);
    let minor: u64 = process_fields[7].parse().unwrap();
    let major: u64 = process_fields[9].parse().unwrap();
    let mut entries: Vec<_> = fs::read_dir("/proc/self/task")
        .unwrap()
        .map(Result::unwrap)
        .collect();
    entries.sort_by_key(|entry| entry.file_name().to_string_lossy().parse::<u64>().unwrap());
    let mut observations = Vec::new();
    let mut complete = true;
    for entry in entries {
        let Ok(stat) = fs::read_to_string(entry.path().join("stat")) else {
            complete = false;
            continue;
        };
        let Ok(sched) = fs::read_to_string(entry.path().join("schedstat")) else {
            complete = false;
            continue;
        };
        let f = fields(&stat);
        let tid: u64 = entry.file_name().to_string_lossy().parse().unwrap();
        let identity = (tid, f[19].parse::<u64>().unwrap());
        let slot = {
            let mut slots = SLOTS
                .get_or_init(|| Mutex::new(BTreeMap::new()))
                .lock()
                .unwrap();
            let next = slots.len();
            *slots.entry(identity).or_insert(next)
        };
        let s: Vec<u64> = sched
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect();
        assert_eq!(s.len(), 3);
        let cpu: u64 = f[36].parse().unwrap();
        let minflt: u64 = f[7].parse().unwrap();
        let majflt: u64 = f[9].parse().unwrap();
        observations.push(format!("{{\"slot\":{slot},\"measuring_thread\":{},\"cpu\":{cpu},\"minor_faults\":{minflt},\"major_faults\":{majflt},\"run_ns\":{},\"wait_ns\":{},\"timeslices\":{}}}",tid == own_tid,s[0],s[1],s[2]));
    }
    format!(
        "{{\"available\":true,\"complete\":{complete},\"minor_faults\":{minor},\"major_faults\":{major},\"snapshot_ns\":{},\"threads\":[{}]}}",
        start.elapsed().as_nanos(),
        observations.join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::fields;

    #[test]
    fn stat_offsets_survive_spaces_and_parentheses_in_comm() {
        let mut values = vec!["0"; 40];
        values[7] = "19";
        values[9] = "3";
        values[19] = "101";
        values[36] = "27";
        let stat = format!("777 (worker ) tricky) {}", values.join(" "));
        let parsed = fields(&stat);
        assert_eq!(
            (parsed[7], parsed[9], parsed[19], parsed[36]),
            ("19", "3", "101", "27")
        );
    }
}
