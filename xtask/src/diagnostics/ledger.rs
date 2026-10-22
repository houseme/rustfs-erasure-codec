//! Shared numeric, availability and residency rules for native diagnostic ledgers.
use crate::Result;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

// Nullable artifact fields are required, so omission cannot masquerade as null.
pub(super) fn nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> std::result::Result<Option<T>, D::Error> {
    Option::deserialize(d)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Snapshot {
    pub status: String,
    #[serde(deserialize_with = "nullable")]
    pub pages: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    pub resident_pages: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    pub errno: Option<u64>,
}
impl Snapshot {
    pub fn validate(
        &self,
        size: u64,
        offset: Option<u64>,
        page: u64,
        native: bool,
        former: bool,
        unsupported_empty: bool,
    ) -> Result<()> {
        if page == 0
            || size > 0 && offset.is_none_or(|n| n >= page)
            || size == 0 && offset.is_some()
        {
            return Err("invalid residency offset/page".into());
        }
        let pages = if size > 0 {
            (u128::from(offset.ok_or("missing residency offset")?) + u128::from(size))
                .div_ceil(u128::from(page))
        } else {
            0
        };
        let valid = match self.status.as_str() {
            "unsupported" if !native && (size > 0 || unsupported_empty) => {
                self.pages.is_none() && self.resident_pages.is_none() && self.errno.is_none()
            }
            "empty" if size == 0 => {
                self.pages == Some(0) && self.resident_pages == Some(0) && self.errno.is_none()
            }
            "mapped" if size > 0 => {
                self.pages.is_some_and(|n| u128::from(n) == pages)
                    && self
                        .resident_pages
                        .zip(self.pages)
                        .is_some_and(|(n, total)| n <= total)
                    && self.errno.is_none()
            }
            "not_fully_mapped" if former && size > 0 => {
                self.pages.is_some_and(|n| u128::from(n) == pages)
                    && self.resident_pages.is_none()
                    && self.errno == Some(12)
            }
            _ => false,
        };
        if !valid {
            return Err("invalid live/former residency snapshot".into());
        }
        Ok(())
    }
}

pub(super) fn offsets(value: Option<&[u64; 3]>, size: u64, page: u64) -> Result<()> {
    if page == 0 {
        return Err("invalid base page size".into());
    }
    match value {
        None if size == 0 => Ok(()),
        Some(p)
            if size > 0
                && p.iter().zip([64, 4096, page]).all(|(n, m)| *n < m)
                && p[1] % 64 == p[0]
                && p[2] % page.min(4096) == p[1] % page.min(4096) =>
        {
            Ok(())
        }
        _ => Err("invalid buffer offsets or dangling empty storage".into()),
    }
}

#[derive(Default)]
pub(super) struct Clock {
    elapsed: Option<u64>,
    faults: Option<[u64; 2]>,
    available: Option<bool>,
}
impl Clock {
    pub fn observe(&mut self, elapsed: u64, faults: Option<[u64; 2]>, native: bool) -> Result<()> {
        if self.elapsed.is_some_and(|previous| elapsed < previous) {
            return Err("nonmonotonic source boundary".into());
        }
        if self.available.is_some_and(|v| v != faults.is_some()) {
            return Err("changing source counter availability".into());
        }
        if native && faults.is_none() {
            return Err("missing native source fault counters".into());
        }
        if let (Some(previous), Some(current)) = (self.faults, faults)
            && previous.iter().zip(current).any(|(a, b)| *a > b)
        {
            return Err("invalid source fault counters".into());
        }
        self.elapsed = Some(elapsed);
        self.available = Some(faults.is_some());
        self.faults = faults;
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum Request {
    VerifyScratch {
        data: super::verify_scratch::Ledger,
        case: String,
        iterations: u64,
        page: u64,
        #[serde(default)]
        native: bool,
    },
    StreamCore {
        data: super::stream_core::Ledger,
        case: String,
        iterations: u64,
        page: u64,
        current: bool,
        #[serde(default)]
        native: bool,
    },
}
impl Request {
    pub(crate) fn execute(self) -> Result<Value> {
        match self {
            Self::VerifyScratch {
                data,
                case,
                iterations,
                page,
                native,
            } => data.validate(&case, iterations, page, native),
            Self::StreamCore {
                data,
                case,
                iterations,
                page,
                current,
                native,
            } => data.validate(&case, iterations, page, current, native),
        }
    }
}
pub(crate) fn run(args: &crate::Args) -> Result<()> {
    let request: Request =
        crate::artifact::read_json(std::path::Path::new(args.required("--input")?))?;
    println!("{}", serde_json::to_string(&request.execute()?)?);
    Ok(())
}

#[cfg(test)]
mod tests;
