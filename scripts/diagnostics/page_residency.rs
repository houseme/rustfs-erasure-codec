// Diagnostic-only Linux virtual-page residency. No buffer memory is read, written,
// prefaulted, locked, or dereferenced, including queries of former virtual ranges.
// A mapped result is a momentary page-cache/residency snapshot, not RSS, physical
// ownership, allocation identity or a promise that a later access cannot fault.
use std::fmt;

const MAX_PAGES: usize = 4097;
#[derive(Clone, Copy)]
pub(super) struct Snapshot {
    status: &'static str,
    pages: Option<usize>,
    resident: Option<usize>,
    errno: Option<i32>,
}
impl Snapshot {
    pub(super) const EMPTY: Self = Self {
        status: "empty",
        pages: Some(0),
        resident: Some(0),
        errno: None,
    };
}
impl fmt::Display for Snapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{\"status\":\"{}\",\"pages\":", self.status)?;
        match self.pages {
            Some(n) => write!(f, "{n}")?,
            None => f.write_str("null")?,
        }
        f.write_str(",\"resident_pages\":")?;
        match self.resident {
            Some(n) => write!(f, "{n}")?,
            None => f.write_str("null")?,
        }
        f.write_str(",\"errno\":")?;
        match self.errno {
            Some(n) => write!(f, "{n}")?,
            None => f.write_str("null")?,
        }
        f.write_str("}")
    }
}
#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn getpagesize() -> std::ffi::c_int;
    fn mincore(addr: *mut std::ffi::c_void, length: usize, vec: *mut u8) -> std::ffi::c_int;
}
pub(super) fn page_size() -> Option<usize> {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: getpagesize has no arguments or memory preconditions.
        let size = usize::try_from(unsafe { getpagesize() }).ok()?;
        size.is_power_of_two().then_some(size)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}
pub(super) fn query(address: usize, bytes: usize, page: usize) -> Snapshot {
    if bytes == 0 {
        return Snapshot::EMPTY;
    }
    #[cfg(target_os = "linux")]
    {
        // Never size a kernel output buffer using the controller's assertion
        // alone. The actual OS page size is checked on every safe entry.
        if page_size() != Some(page) {
            return Snapshot {
                status: "invalid_range",
                pages: None,
                resident: None,
                errno: None,
            };
        }
        let offset = address % page;
        let Some(span) = offset
            .checked_add(bytes)
            .filter(|_| address.checked_add(bytes).is_some())
        else {
            return Snapshot {
                status: "invalid_range",
                pages: None,
                resident: None,
                errno: None,
            };
        };
        let pages = span.div_ceil(page);
        if pages > MAX_PAGES {
            return Snapshot {
                status: "over_budget",
                pages: Some(pages),
                resident: None,
                errno: None,
            };
        }
        let mut bitmap = [0u8; MAX_PAGES];
        // SAFETY: addr is page-aligned, length cannot overflow the checked range,
        // and bitmap has >= ceil(length/actual OS page size) writable bytes.
        // mincore only queries this virtual range; it does not dereference addr
        // in Rust or access its contents. Unmapped/former ranges are legal inputs
        // and return errno; even a partially unmapped range has no usable count.
        let result = unsafe {
            mincore(
                std::ptr::without_provenance_mut(address - offset),
                span,
                bitmap.as_mut_ptr(),
            )
        };
        if result == 0 {
            Snapshot {
                status: "mapped",
                pages: Some(pages),
                resident: Some(bitmap[..pages].iter().filter(|v| **v & 1 != 0).count()),
                errno: None,
            }
        } else {
            let errno = std::io::Error::last_os_error().raw_os_error();
            Snapshot {
                status: if errno == Some(12) {
                    "not_fully_mapped"
                } else {
                    "query_failed"
                },
                pages: Some(pages),
                resident: None,
                errno,
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (address, page, MAX_PAGES);
        Snapshot {
            status: "unsupported",
            pages: None,
            resident: None,
            errno: None,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_and_live_storage_have_explicit_native_coverage() {
        let empty = query(1, 0, 1);
        assert_eq!(empty.status, "empty");
        let source = vec![0x57u8; 16387];
        match page_size() {
            Some(page) => {
                let snapshot = query(source.as_ptr() as usize, source.len(), page);
                assert_eq!(snapshot.status, "mapped");
                assert_eq!(snapshot.resident, snapshot.pages);
                assert_eq!(
                    query(source.as_ptr() as usize, source.len(), page * 2).status,
                    "invalid_range"
                );
                assert_eq!(
                    query(usize::MAX - page, page * 2, page).status,
                    "invalid_range"
                );
                assert_eq!(
                    query(page, page * (MAX_PAGES + 1), page).status,
                    "over_budget"
                );
                // No assumptions about address availability or allocator retention.
                let address = source.as_ptr() as usize;
                drop(source);
                assert!(matches!(
                    query(address, 16387, page).status,
                    "mapped" | "not_fully_mapped"
                ));
            }
            None => assert_eq!(
                query(source.as_ptr() as usize, source.len(), 4096).status,
                "unsupported"
            ),
        }
    }
}
