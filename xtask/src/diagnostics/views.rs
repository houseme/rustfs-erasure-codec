//! One typed ledger for stream I/O, lifecycle windows and virtual-page snapshots.
use super::ledger::{Snapshot, nullable};
use crate::Result;
use serde::{Deserialize, Serialize, ser::SerializeMap};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const LIMIT: usize = 4096;
const ENTRIES: [(&str, &str); 3] = [
    (
        "encode_stream",
        "Vec<stream_views::Reader<'_>> -> Vec<stream_views::Writer>",
    ),
    ("verify_stream", "Vec<stream_views::Reader<'_>>"),
    ("reconstruct_stream", "Vec<Cursor<Vec<u8>>>"),
];
const COUNTERS: [&str; 5] = [
    "wall_ns",
    "process_minor_faults",
    "process_major_faults",
    "calling_thread_run_ns",
    "calling_thread_wait_ns",
];

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum Role {
    RetainedSource,
    SourceReadView,
    ReadScratchView,
    ParityWriteView,
    WriterBefore,
    WriterAfter,
    CursorBeforeMissing,
    CursorAfterRecovery,
    CursorMissing,
}
impl Role {
    fn owned(self) -> bool {
        !matches!(
            self,
            Self::SourceReadView | Self::ReadScratchView | Self::ParityWriteView
        )
    }
    fn writer(self) -> bool {
        matches!(
            self,
            Self::ParityWriteView | Self::WriterBefore | Self::WriterAfter
        )
    }
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Call,
    Reserve,
    Append,
    Drop,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Boundary {
    View,
    BeforeRead,
    AfterReserve,
    AfterApiReturn,
    AfterWriterDrop,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Event {
    call: u64,
    role: Role,
    index: u64,
    len: u64,
    #[serde(deserialize_with = "nullable")]
    capacity: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    offsets: Option<[u64; 3]>,
    bytes: u64,
    #[serde(deserialize_with = "nullable")]
    same_address_as_previous_view: Option<bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Window {
    call: u64,
    phase: Phase,
    #[serde(deserialize_with = "nullable")]
    index: Option<u64>,
    bytes: u64,
    start_ns: u64,
    end_ns: u64,
    #[serde(deserialize_with = "nullable")]
    before: Option<[u64; 4]>,
    #[serde(deserialize_with = "nullable")]
    after: Option<[u64; 4]>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Residency {
    call: u64,
    boundary: Boundary,
    role: Role,
    index: u64,
    bytes: u64,
    #[serde(deserialize_with = "nullable")]
    offset_page: Option<u64>,
    snapshot: Snapshot,
}
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Position {
    len: u64,
    capacity: u64,
    offset_64: u64,
    offset_4096: u64,
    offset_page: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct View {
    schema: String,
    observation: u64,
    iterations: u64,
    calls: u64,
    completed: u64,
    page_size: u64,
    overflow: u64,
    release_approval: bool,
    events: Vec<Event>,
    windows: Vec<Window>,
    residency: Vec<Residency>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Observation<V = View> {
    case: String,
    iterations: u64,
    page_size: u64,
    observation: u64,
    view: V,
    source_fixture: Option<Vec<Position>>,
    #[serde(default)]
    require_counters: bool,
}
#[derive(Deserialize)]
pub(crate) struct WindowView {
    calls: u64,
    windows: Vec<Window>,
}
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum Request {
    Entries,
    Underlying {
        records: Vec<Value>,
    },
    Inventory {
        observations: Vec<Observation>,
    },
    Validate {
        observations: Vec<Observation>,
        #[serde(default)]
        summary: bool,
    },
    Summarize {
        view: WindowView,
    },
}
impl Request {
    pub(crate) fn execute(self) -> Result<Value> {
        match self {
            Self::Entries => Ok(serde_json::to_value(BTreeMap::from(ENTRIES))?),
            Self::Underlying { mut records } => {
                underlying_rows(&mut records)?;
                Ok(Value::Array(records))
            }
            Self::Inventory { observations } => {
                let reports = observations
                    .iter()
                    .map(|o| {
                        let calls = validate(o, true).map_err(|e| {
                            format!("{} observation {}: {e}", o.case, o.observation)
                        })?;
                        let shapes: BTreeSet<_> = o
                            .view
                            .events
                            .iter()
                            .map(|e| ViewShape {
                                role: e.role,
                                index: e.index,
                                len: e.len,
                                capacity: e.capacity,
                                offsets: e.offsets,
                            })
                            .collect();
                        Ok(json!({"case":o.case,"observation":o.observation,"calls":calls,"shapes":shapes}))
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(Value::Array(reports))
            }
            Self::Validate {
                observations,
                summary,
            } => {
                let reports = observations
                    .iter()
                    .map(|o| {
                        validate(o, summary).map_err(|e| {
                            format!("{} observation {}: {e}", o.case, o.observation).into()
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(Value::Array(reports))
            }
            Self::Summarize { view } => summarize(view.calls, &view.windows),
        }
    }
}

/// Compact shapes from the validated typed ledger; no second event JSON model.
#[derive(Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub(crate) struct ViewShape {
    role: Role,
    index: u64,
    len: u64,
    #[serde(deserialize_with = "nullable")]
    capacity: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    offsets: Option<[u64; 3]>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Signature {
    pub case: String,
    pub observation: u64,
    pub shapes: BTreeSet<ViewShape>,
}

pub(super) fn underlying_rows(records: &mut [Value]) -> Result<()> {
    for record in records {
        let case = record["case"].as_str().ok_or("missing stream audit case")?;
        let operation = case.split(':').next().ok_or("missing stream operation")?;
        let entry = ENTRIES
            .iter()
            .find(|(name, _)| *name == operation)
            .map(|(_, entry)| *entry)
            .ok_or("unknown stream audit operation")?;
        let contract = crate::sampling_contract::fixture_contract("stream", case)?;
        for key in ["fixture", "workload"] {
            let value = record
                .get_mut(key)
                .and_then(Value::as_object_mut)
                .ok_or("missing actual stream audit entry")?;
            if value.get("call_entry").and_then(Value::as_str) != Some(entry) {
                return Err("missing actual stream audit entry".into());
            }
            value.insert("call_entry".into(), contract["call_entry"].clone());
        }
    }
    Ok(())
}

pub(super) fn worker(
    views: &[View],
    bindings: &super::worker::Bindings<'_>,
    case: &str,
    page: u64,
) -> Result<()> {
    if views.len() != 12 {
        return Err("incomplete original stream worker".into());
    }
    for (index, view) in views.iter().enumerate() {
        let source = bindings.rows[index]["fixture"]["source"].clone();
        let observation = Observation {
            case: case.to_owned(),
            iterations: 2,
            page_size: page,
            observation: index as u64,
            view,
            source_fixture: Some(serde_json::from_value(source)?),
            require_counters: bindings.native,
        };
        validate(&observation, false)?;
        if bindings.native {
            for window in &view.windows {
                for counters in window.before.iter().chain(&window.after) {
                    bindings.enclose(
                        index as u64,
                        counters,
                        &[
                            "process_minor_faults",
                            "process_major_faults",
                            "run_ns",
                            "wait_ns",
                        ],
                    )?;
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn run(args: &crate::Args) -> Result<()> {
    let request: Request =
        crate::artifact::read_json(std::path::Path::new(args.required("--input")?))?;
    println!("{}", serde_json::to_string(&request.execute()?)?);
    Ok(())
}

struct Shape {
    operation: String,
    data: u64,
    parity: u64,
    size: u64,
    block: u64,
    sources: usize,
}
impl Shape {
    fn parse(case: &str) -> Result<Self> {
        let parts: Vec<_> = case.split(':').collect();
        if parts.len() != 5 || !ENTRIES.iter().any(|(name, _)| *name == parts[0]) {
            return Err("invalid stream audit shape".into());
        }
        let (data, parity, size, block): (u64, u64, u64, u64) = (
            parts[1].parse()?,
            parts[2].parse()?,
            parts[3].parse()?,
            parts[4].parse()?,
        );
        let total = data
            .checked_add(parity)
            .ok_or("invalid stream audit shape")?;
        if data == 0
            || parity == 0
            || total > 48
            || size == 0
            || !(1024..=16777216).contains(&block)
        {
            return Err("invalid stream audit shape".into());
        }
        Ok(Self {
            operation: parts[0].into(),
            data,
            parity,
            size,
            block,
            sources: if parts[0] == "encode_stream" {
                data
            } else {
                total
            } as usize,
        })
    }
}
fn bounded(len: usize) -> Result<()> {
    if !(1..=LIMIT).contains(&len) {
        return Err("missing bounded stream ledger".into());
    }
    Ok(())
}
fn by_call<T>(values: &[T], calls: u64, call: impl Fn(&T) -> u64) -> Result<Vec<&[T]>> {
    if calls == 0 || calls > LIMIT as u64 {
        return Err("invalid bounded lifecycle calls".into());
    }
    let mut cursor = 0;
    let mut result = Vec::with_capacity(calls as usize + 1);
    for index in 0..=calls {
        let start = cursor;
        while cursor < values.len() && call(&values[cursor]) == index {
            cursor += 1;
        }
        result.push(&values[start..cursor]);
    }
    if cursor != values.len() {
        return Err("stream calls reordered or out of range".into());
    }
    Ok(result)
}
fn ordered(before: &[u64; 4], after: &[u64; 4]) -> bool {
    before.iter().zip(after).all(|(a, b)| a <= b)
}

fn validate<V: std::borrow::Borrow<View>>(o: &Observation<V>, summary: bool) -> Result<Value> {
    let v = o.view.borrow();
    if v.schema != "stream-buffer-views-v3" || v.release_approval {
        return Err("invalid stream view schema".into());
    }
    if v.observation != o.observation
        || v.iterations != o.iterations
        || v.calls != o.iterations
        || v.completed != o.iterations
        || v.page_size != o.page_size
        || v.overflow != 0
        || o.page_size == 0
    {
        return Err("incomplete call/drop schedule or bounded view overflow".into());
    }
    let s = Shape::parse(&o.case)?;
    for len in [v.events.len(), v.windows.len(), v.residency.len()] {
        bounded(len)?;
    }
    // Each ledger is partitioned once. All later checks borrow these slices.
    let events = by_call(&v.events, o.iterations, |e| e.call)?;
    let windows = by_call(&v.windows, o.iterations, |w| w.call)?;
    let residency = by_call(&v.residency, o.iterations, |r| r.call)?;
    for e in &v.events {
        let _comparison = e.same_address_as_previous_view; // Explicit nullable bool; never an inferred allocation identity.
        if e.role.owned() && e.capacity.is_none_or(|cap| cap < e.len)
            || !e.role.owned() && e.capacity.is_some()
        {
            return Err("owned capacity must not be inferred from a borrowed view".into());
        }
        super::ledger::offsets(e.offsets.as_ref(), e.capacity.unwrap_or(e.len), o.page_size)?;
        let limit = if e.role.writer() {
            s.parity
        } else {
            s.sources as u64
        };
        if e.index >= limit || e.bytes > e.len && e.role != Role::WriterAfter {
            return Err("stream role/index or transfer mismatch".into());
        }
    }
    let retained = events[0];
    if retained.len() != s.sources
        || retained
            .iter()
            .enumerate()
            .any(|(i, e)| e.index != i as u64 || e.role != Role::RetainedSource || e.len != s.size)
    {
        return Err("missing retained source views".into());
    }
    if let Some(expected) = &o.source_fixture {
        let positions = retained
            .iter()
            .map(|e| {
                let p = e.offsets.ok_or("missing retained offsets")?;
                Ok(Position {
                    len: e.len,
                    capacity: e.capacity.ok_or("missing retained capacity")?,
                    offset_64: p[0],
                    offset_4096: p[1],
                    offset_page: p[2],
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if positions != *expected {
            return Err("stream view source differs from retained caller fixture".into());
        }
    }
    validate_windows(&v.windows, &windows, &events, &s, o.require_counters)?;
    validate_residency(
        &v.residency,
        &residency,
        &events,
        &s,
        o.page_size,
        o.require_counters,
    )?;
    for current in &events[1..] {
        validate_io(current, retained, &s, o.page_size)?;
    }
    if summary {
        summarize_indexed(&windows)
    } else {
        Ok(Value::Null)
    }
}

fn validate_windows(
    all: &[Window],
    calls: &[&[Window]],
    events: &[&[Event]],
    shape: &Shape,
    native: bool,
) -> Result<()> {
    if !calls[0].is_empty() {
        return Err("invalid lifecycle call zero".into());
    }
    let available = all[0].before.is_some();
    if native && !available {
        return Err("native Linux lifecycle counters are missing".into());
    }
    for w in all {
        if w.end_ns < w.start_ns
            || w.before.is_some() != w.after.is_some()
            || w.before.is_some() != available
        {
            return Err("invalid lifecycle counters/time or changing availability".into());
        }
        if let (Some(a), Some(b)) = (&w.before, &w.after)
            && !ordered(a, b)
        {
            return Err("decreasing lifecycle counters".into());
        }
        if w.phase == Phase::Append {
            if w.index.is_none_or(|i| i >= shape.parity) {
                return Err("wrong append shard".into());
            }
        } else if w.index.is_some() {
            return Err("unexpected lifecycle shard".into());
        }
    }
    let mut previous_end = 0;
    for (call, current) in calls.iter().enumerate().skip(1) {
        let (parent, children) = current
            .split_last()
            .ok_or("missing completed lifecycle call")?;
        if parent.phase != Phase::Call || parent.bytes != 0 || parent.start_ns < previous_end {
            return Err("overlapping or missing lifecycle calls".into());
        }
        previous_end = parent.end_ns;
        if shape.operation != "encode_stream" {
            if !children.is_empty() {
                return Err("unexpected writer lifecycle".into());
            }
            continue;
        }
        let count = u128::from(shape.parity) * u128::from(shape.size.div_ceil(shape.block));
        if children.len() as u128 != count + 2
            || children[0].phase != Phase::Reserve
            || children[children.len() - 1].phase != Phase::Drop
            || children[1..children.len() - 1]
                .iter()
                .any(|w| w.phase != Phase::Append)
        {
            return Err("incomplete reserve/append/drop lifecycle".into());
        }
        let bytes = u128::from(shape.parity) * u128::from(shape.size);
        if u128::from(children[0].bytes) != bytes
            || u128::from(children[children.len() - 1].bytes) != bytes
        {
            return Err("writer lifecycle byte total changed".into());
        }
        let appends: Vec<_> = events[call]
            .iter()
            .filter(|e| e.role == Role::ParityWriteView)
            .map(|e| (Some(e.index), e.bytes))
            .collect();
        if !children[1..children.len() - 1]
            .iter()
            .map(|w| (w.index, w.bytes))
            .eq(appends)
        {
            return Err("append windows differ from actual I/O views".into());
        }
        let mut last = parent.start_ns;
        let mut counters = &parent.before;
        for child in children {
            if child.start_ns < last || child.end_ns > parent.end_ns {
                return Err("overlapping or escaping writer windows".into());
            }
            if let (Some(a), Some(b), Some(c), Some(d)) =
                (counters, &child.before, &child.after, &parent.after)
                && (!ordered(a, b) || !ordered(c, d))
            {
                return Err("writer counters escape the enclosing call".into());
            }
            last = child.end_ns;
            counters = &child.after;
        }
    }
    Ok(())
}

type Identity = (u64, Role, u64, u64, Option<u64>);
fn identity(r: &Residency) -> Identity {
    (r.call, r.role, r.index, r.bytes, r.offset_page)
}
fn event_identity(e: &Event) -> Identity {
    (
        e.call,
        e.role,
        e.index,
        e.capacity.unwrap_or(e.len),
        e.offsets.map(|p| p[2]),
    )
}
fn sorted(mut values: Vec<Identity>) -> Vec<Identity> {
    values.sort_unstable();
    values
}
fn validate_residency(
    all: &[Residency],
    calls: &[&[Residency]],
    events: &[&[Event]],
    shape: &Shape,
    page: u64,
    native: bool,
) -> Result<()> {
    for r in all {
        let former = matches!(
            r.boundary,
            Boundary::AfterApiReturn | Boundary::AfterWriterDrop
        );
        r.snapshot
            .validate(r.bytes, r.offset_page, page, native, former, true)
            .map_err(|e| format!("native residency coverage failed: {e}"))?;
    }
    for (call, current) in calls.iter().enumerate() {
        let actual = events[call];
        if !current
            .iter()
            .filter(|r| r.boundary == Boundary::View)
            .map(identity)
            .eq(actual.iter().map(event_identity))
        {
            return Err("residency differs from actual view schedule".into());
        }
        let before: Vec<_> = current
            .iter()
            .filter(|r| r.boundary == Boundary::BeforeRead)
            .map(identity)
            .collect();
        let reads: Vec<_> = actual
            .iter()
            .filter(|e| e.role == Role::ReadScratchView)
            .map(|e| (e.call, e.role, e.index, e.len, e.offsets.map(|p| p[2])))
            .collect();
        if sorted(before) != sorted(reads) {
            return Err("missing before-read residency".into());
        }
        let mut pending = BTreeSet::new();
        for r in *current {
            if r.boundary == Boundary::BeforeRead && !pending.insert(r.index) {
                return Err("overlapping read for one shard".into());
            }
            if r.boundary == Boundary::View
                && r.role == Role::ReadScratchView
                && !pending.remove(&r.index)
            {
                return Err("read snapshot without preceding boundary".into());
            }
        }
        if !pending.is_empty() {
            return Err("unfinished read residency boundary".into());
        }
        let extra: Vec<_> = current
            .iter()
            .filter(|r| !matches!(r.boundary, Boundary::View | Boundary::BeforeRead))
            .collect();
        if shape.operation != "encode_stream" || call == 0 {
            if !extra.is_empty() {
                return Err("unexpected residency boundary".into());
            }
            continue;
        }
        let reserved: Vec<_> = extra
            .iter()
            .filter(|r| r.boundary == Boundary::AfterReserve)
            .collect();
        if reserved.len() != shape.parity as usize
            || reserved
                .iter()
                .enumerate()
                .any(|(i, r)| r.index != i as u64)
        {
            return Err("incomplete reserved-writer residency".into());
        }
        for r in reserved {
            let first = actual
                .iter()
                .find(|e| e.role == Role::WriterBefore && e.index == r.index)
                .ok_or("missing reserved writer view")?;
            if identity(r) != event_identity(first) {
                return Err("reserved range changed before write".into());
            }
        }
        let last_views = |roles: &[Role]| {
            let mut last = BTreeMap::new();
            for e in actual {
                if roles.contains(&e.role) {
                    last.insert((e.role, e.index), event_identity(e));
                }
            }
            sorted(last.into_values().collect())
        };
        let returned: Vec<_> = extra
            .iter()
            .filter(|r| r.boundary == Boundary::AfterApiReturn)
            .map(|r| identity(r))
            .collect();
        let dropped: Vec<_> = extra
            .iter()
            .filter(|r| r.boundary == Boundary::AfterWriterDrop)
            .map(|r| identity(r))
            .collect();
        let returned_len = returned.len();
        if sorted(returned) != last_views(&[Role::ReadScratchView, Role::ParityWriteView])
            || sorted(dropped) != last_views(&[Role::WriterAfter])
        {
            return Err("missing former scratch/writer ranges".into());
        }
        let expected: Vec<_> = std::iter::repeat_n(Boundary::AfterReserve, shape.parity as usize)
            .chain(std::iter::repeat_n(Boundary::AfterApiReturn, returned_len))
            .chain(std::iter::repeat_n(
                Boundary::AfterWriterDrop,
                shape.parity as usize,
            ))
            .collect();
        if !extra.iter().map(|r| r.boundary).eq(expected)
            || !current
                .iter()
                .take(shape.parity as usize)
                .all(|r| r.boundary == Boundary::AfterReserve)
            || !current
                .iter()
                .rev()
                .take(shape.parity as usize)
                .all(|r| r.boundary == Boundary::AfterWriterDrop)
            || current
                .iter()
                .rev()
                .skip(shape.parity as usize)
                .take(returned_len)
                .any(|r| r.boundary != Boundary::AfterApiReturn)
        {
            return Err("residency release boundaries reordered".into());
        }
    }
    Ok(())
}

#[derive(Default)]
struct Reader<'a> {
    source: Vec<&'a Event>,
    scratch: Vec<&'a Event>,
}
#[derive(Default)]
struct Writer<'a> {
    input: Vec<&'a Event>,
    before: Vec<&'a Event>,
    after: Vec<&'a Event>,
}
fn validate_io(current: &[Event], retained: &[Event], shape: &Shape, page: u64) -> Result<()> {
    if shape.operation == "reconstruct_stream" {
        if current.len() != 3 * shape.sources {
            return Err("unexpected cursor observations".into());
        }
        for (part, role) in current.chunks_exact(shape.sources).zip([
            Role::CursorBeforeMissing,
            Role::CursorMissing,
            Role::CursorAfterRecovery,
        ]) {
            let indices: BTreeSet<_> = part.iter().map(|e| e.index).collect();
            if indices.len() != shape.sources || part.iter().any(|e| e.role != role) {
                return Err("missing or reordered cursor stages".into());
            }
            for e in part {
                let missing = role == Role::CursorMissing && [0, shape.data].contains(&e.index);
                if e.len != if missing { 0 } else { shape.size } || missing && e.capacity != Some(0)
                {
                    return Err("wrong missing/recovered cursor storage".into());
                }
            }
        }
        return Ok(());
    }
    let mut readers: Vec<Reader<'_>> = (0..shape.sources).map(|_| Reader::default()).collect();
    let mut writers: Vec<Writer<'_>> = (0..shape.parity).map(|_| Writer::default()).collect();
    let mut previous_read: Option<&Event> = None;
    for e in current {
        match e.role {
            Role::SourceReadView => {
                if previous_read.replace(e).is_some() {
                    return Err("reader/scratch callback pairs changed".into());
                }
                readers[e.index as usize].source.push(e);
            }
            Role::ReadScratchView => {
                let before = previous_read.take().ok_or("missing source read callback")?;
                if before.index != e.index || before.bytes != e.bytes {
                    return Err("reader/scratch callback pairs changed".into());
                }
                readers[e.index as usize].scratch.push(e);
            }
            role if shape.operation == "encode_stream" => match role {
                Role::ParityWriteView => writers[e.index as usize].input.push(e),
                Role::WriterBefore => writers[e.index as usize].before.push(e),
                Role::WriterAfter => writers[e.index as usize].after.push(e),
                _ => return Err("unexpected encode view".into()),
            },
            _ => return Err("unexpected verify view".into()),
        }
    }
    if previous_read.is_some() {
        return Err("unfinished source read callback".into());
    }
    for (index, reader) in readers.iter().enumerate() {
        for (values, scratch) in [(&reader.source, false), (&reader.scratch, true)] {
            if values.is_empty()
                || values.iter().map(|e| u128::from(e.bytes)).sum::<u128>()
                    != u128::from(shape.size)
                || !values.iter().any(|e| e.bytes == 0)
                || scratch && values.iter().any(|e| e.len > shape.block)
            {
                return Err("incomplete reader/scratch transfer".into());
            }
        }
        let origin = retained[index]
            .offsets
            .ok_or("missing source backing offsets")?;
        let mut remaining = shape.size;
        for e in &reader.source {
            if e.len != remaining {
                return Err("source prefix consumption changed".into());
            }
            let mods = [64, 4096, page];
            let expected: [u64; 3] = std::array::from_fn(|i| {
                ((u128::from(origin[i]) + u128::from(shape.size - remaining)) % u128::from(mods[i]))
                    as u64
            });
            if remaining > 0
                && e.offsets.as_ref().map(|p| p.as_slice()) != Some(expected.as_slice())
            {
                return Err("source view moved outside retained backing".into());
            }
            remaining -= e.bytes;
        }
    }
    if shape.operation == "verify_stream" {
        return Ok(());
    }
    for writer in writers {
        if writer.input.is_empty()
            || writer.input.len() != writer.before.len()
            || writer.before.len() != writer.after.len()
            || writer
                .input
                .iter()
                .map(|e| u128::from(e.bytes))
                .sum::<u128>()
                != u128::from(shape.size)
        {
            return Err("incomplete parity/writer transfer".into());
        }
        let mut previous = 0;
        for ((source, old), new) in writer.input.iter().zip(writer.before).zip(writer.after) {
            if source.len == 0
                || source.len > shape.block
                || old.len != previous
                || old.len.checked_add(source.bytes) != Some(new.len)
                || new.bytes != source.bytes
                || old.capacity.is_none_or(|c| c < shape.size)
                || old.capacity != new.capacity
                || old.offsets != new.offsets
            {
                return Err("writer reservation or append lifecycle changed".into());
            }
            previous = new.len;
        }
        if previous != shape.size {
            return Err("incomplete writer output".into());
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Metrics([Option<u64>; 5]);
impl Serialize for Metrics {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(5))?;
        for (key, value) in COUNTERS.into_iter().zip(self.0) {
            map.serialize_entry(key, &value)?;
        }
        map.end()
    }
}
fn totals<'a>(windows: impl Iterator<Item = &'a Window>) -> Result<Metrics> {
    let mut result = [Some(0_u64); 5];
    for w in windows {
        let wall = w
            .end_ns
            .checked_sub(w.start_ns)
            .ok_or("decreasing lifecycle time")?;
        let mut values = [None; 5];
        values[0] = Some(wall);
        if let (Some(before), Some(after)) = (w.before, w.after) {
            for i in 0..4 {
                values[i + 1] = Some(
                    after[i]
                        .checked_sub(before[i])
                        .ok_or("decreasing lifecycle counters")?,
                );
            }
        }
        for i in 0..5 {
            result[i] = match (result[i], values[i]) {
                (Some(a), Some(b)) => Some(a.checked_add(b).ok_or("lifecycle summary overflow")?),
                _ => None,
            };
        }
    }
    Ok(Metrics(result))
}
fn summarize(calls: u64, windows: &[Window]) -> Result<Value> {
    if windows.len() > LIMIT {
        return Err("oversized lifecycle summary".into());
    }
    summarize_indexed(&by_call(windows, calls, |w| w.call)?)
}
fn summarize_indexed(calls: &[&[Window]]) -> Result<Value> {
    let mut result = Vec::with_capacity(calls.len() - 1);
    for (call, windows) in calls.iter().enumerate().skip(1) {
        let mut groups = Vec::with_capacity(4);
        for phase in [Phase::Call, Phase::Reserve, Phase::Append, Phase::Drop] {
            groups.push(totals(windows.iter().filter(|w| w.phase == phase))?);
        }
        let mut outside = groups[0];
        for child in &groups[1..] {
            for i in 0..5 {
                if let Some(value) = outside.0[i] {
                    outside.0[i] = Some(
                        value
                            .checked_sub(
                                child.0[i].ok_or("partial lifecycle counter availability")?,
                            )
                            .ok_or("writer counters escape call")?,
                    );
                }
            }
        }
        result.push(json!({"call":call,"windows":{"call":groups[0],"reserve":groups[1],"append":groups[2],"drop":groups[3],"outside_writer_windows":outside}}));
    }
    Ok(Value::Array(result))
}

#[cfg(test)]
mod tests;
