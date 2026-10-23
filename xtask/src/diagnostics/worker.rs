//! Validate a complete diagnostic worker in one native frame and one owned model.
use super::{dynamic_ledger, ledger, stream_core, verify_scratch, views};
use crate::{Result, benchmark_contract, release_rows, sampling_contract};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum Data {
    VerifyScratch {
        ledger: verify_scratch::Ledger,
    },
    StreamCore {
        ledger: stream_core::Ledger,
        views: Vec<views::View>,
        current: bool,
    },
    Dynamic {
        ledger: dynamic_ledger::Ledger,
        legacy: bool,
    },
}
#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Sample,
    Calibrate,
}
fn sample() -> Mode {
    Mode::Sample
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    rows: Vec<Value>,
    data: Data,
    profile: String,
    case: String,
    revision: String,
    page: u64,
    cpus: Option<Vec<u64>>,
    #[serde(default = "sample")]
    mode: Mode,
}
pub(super) struct Bindings<'a> {
    pub rows: &'a [Value],
    pub native: bool,
}
impl Bindings<'_> {
    pub fn enclose(&self, observation: u64, counters: &[u64], fields: &[&str]) -> Result<()> {
        let row = self
            .rows
            .get(observation as usize)
            .ok_or("unknown source observation")?;
        for (counter, field) in counters.iter().zip(fields) {
            let a = row["before"][*field]
                .as_u64()
                .ok_or("missing original observation counters")?;
            let b = row["after"][*field]
                .as_u64()
                .ok_or("missing original observation counters")?;
            if !(a..=b).contains(counter) {
                return Err("source counters escape original observation".into());
            }
        }
        Ok(())
    }
    pub fn faults(&self, observation: u64, faults: Option<[u64; 2]>) -> Result<()> {
        if self.native {
            self.enclose(
                observation,
                &faults.ok_or("missing native source counters")?,
                &["process_minor_faults", "process_major_faults"],
            )
        } else if faults.is_some() {
            Err("unrecorded native source counters".into())
        } else {
            Ok(())
        }
    }
    pub fn fixture(
        &self,
        observation: u64,
        role: &str,
        index: u64,
        len: u64,
        capacity: Option<u64>,
        offsets: Option<[u64; 3]>,
    ) -> Result<()> {
        let p = self
            .rows
            .get(observation as usize)
            .and_then(|r| r["fixture"][role].as_array())
            .and_then(|v| v.get(index as usize))
            .ok_or("missing original caller fixture")?;
        let expected = [
            ("len", Some(len)),
            ("capacity", capacity),
            ("offset_64", offsets.map(|p| p[0])),
            ("offset_4096", offsets.map(|p| p[1])),
            ("offset_page", offsets.map(|p| p[2])),
        ];
        if expected
            .iter()
            .any(|(key, value)| p[*key].as_u64() != *value)
        {
            return Err("dynamic view differs from original retained caller fixture".into());
        }
        Ok(())
    }
}
impl Request {
    pub(crate) fn execute(mut self) -> Result<Value> {
        if self.mode == Mode::Calibrate && !matches!(self.data, Data::Dynamic { .. }) {
            return Err("only dynamic diagnostics use calibration mode".into());
        }
        match &self.data {
            Data::Dynamic { .. } => ledger::unwrap_dynamic_records(&mut self.rows)?,
            Data::StreamCore { .. } => {
                if self.profile != "stream" {
                    return Err("invalid stream worker profile".into());
                }
                views::underlying_rows(&mut self.rows)?;
            }
            _ => {}
        }
        sampling_contract::diagnostic_worker(
            &self.rows,
            &self.profile,
            self.page,
            self.cpus.as_deref(),
            self.mode == Mode::Calibrate,
        )?;
        let contract = release_rows::Request {
            profile: self.profile.clone(),
            rows: Value::Null,
            revision: self.revision.clone(),
            iteration_counts: None,
            default_iterations: Some(2),
            complete: false,
        };
        for row in &self.rows {
            if row["case"] != self.case || row["revision"] != self.revision {
                return Err("wrong original diagnostic workload".into());
            }
            contract.validate_row(&row["rows"][0], &self.case)?;
        }
        benchmark_contract::observations_ref(&self.profile, &self.rows, None, false)?;
        let bindings = Bindings {
            rows: &self.rows,
            native: self.cpus.is_some(),
        };
        let summary = match &self.data {
            Data::VerifyScratch { ledger } => {
                if self
                    .rows
                    .iter()
                    .any(|r| r["fixture"] != self.rows[0]["fixture"])
                {
                    return Err("caller fixture changed inside diagnostic worker".into());
                }
                let report = ledger.validate(&self.case, 2, self.page, bindings.native)?;
                ledger.bind(&bindings)?;
                report
            }
            Data::StreamCore {
                ledger,
                views,
                current,
            } => {
                views::worker(views, &bindings, &self.case, self.page)?;
                let report =
                    ledger.validate(&self.case, 2, self.page, *current, bindings.native)?;
                ledger.bind(&bindings)?;
                report
            }
            Data::Dynamic { ledger, legacy } => {
                let report = ledger.validate(
                    &self.profile,
                    &self.case,
                    self.page,
                    bindings.native,
                    *legacy,
                )?;
                ledger.bind(&bindings)?;
                report
            }
        };
        Ok(json!({"summary":summary,
            "preparation":sampling_contract::preparation::Preparation::new(&self.rows, self.mode == Mode::Calibrate)?.catalog()}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn source_faults_must_be_inside_their_own_observation() {
        let rows = vec![
            json!({"before":{"process_minor_faults":10,"process_major_faults":2},"after":{"process_minor_faults":20,"process_major_faults":3}}),
        ];
        let native = Bindings {
            rows: &rows,
            native: true,
        };
        native.faults(0, Some([10, 2])).unwrap();
        native.faults(0, Some([20, 3])).unwrap();
        for faults in [None, Some([9, 2]), Some([21, 3]), Some([10, 4])] {
            assert!(native.faults(0, faults).is_err());
        }
        assert!(native.faults(1, Some([10, 2])).is_err());
        let portable = Bindings {
            rows: &rows,
            native: false,
        };
        portable.faults(0, None).unwrap();
        assert!(portable.faults(0, Some([10, 2])).is_err());
    }
    #[test]
    fn retained_views_bind_to_actual_caller_geometry() {
        let rows = vec![
            json!({"fixture":{"source":[{"len":1024,"capacity":2048,"offset_64":16,"offset_4096":80,"offset_page":80}]}}),
        ];
        let b = Bindings {
            rows: &rows,
            native: false,
        };
        b.fixture(0, "source", 0, 1024, Some(2048), Some([16, 80, 80]))
            .unwrap();
        assert!(
            b.fixture(0, "source", 0, 1024, Some(1024), Some([16, 80, 80]))
                .is_err()
        );
        assert!(
            b.fixture(0, "source", 1, 1024, Some(2048), Some([16, 80, 80]))
                .is_err()
        );
        assert!(
            b.fixture(0, "source", 0, 1024, Some(2048), Some([16, 80, 81]))
                .is_err()
        );
    }
}
