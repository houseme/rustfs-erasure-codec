use crate::Result;
use serde::de::{Deserialize, DeserializeOwned, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
};
pub(crate) mod snapshot;

pub(crate) fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = std::fs::read(path)?;
    let StrictJson(value) =
        serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_value(value).map_err(|error| format!("{}: {error}", path.display()).into())
}

pub(crate) fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let mut writer = std::io::BufWriter::new(std::fs::File::create(path)?);
    serde_json::to_writer_pretty(&mut writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

pub(crate) fn entries(path: &Path) -> Result<Vec<std::fs::DirEntry>> {
    let reader = match std::fs::read_dir(path) {
        Ok(reader) => reader,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut entries = reader.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    Ok(entries)
}

pub(crate) fn criterion_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(folder) = pending.pop() {
        for entry in entries(&folder)? {
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            } else if entry.file_type()?.is_file()
                && entry.file_name() == "estimates.json"
                && folder.file_name().is_some_and(|name| name == "new")
            {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    Ok(files)
}

pub(crate) fn csv_rows(path: &Path) -> Result<Vec<BTreeMap<String, String>>> {
    let mut reader = csv::Reader::from_path(path)?;
    let header = reader.headers()?.clone();
    if header
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != header.len()
    {
        return Err(format!("{}: duplicate CSV columns", path.display()).into());
    }
    reader
        .deserialize()
        .map(|row| row.map_err(Into::into))
        .collect()
}

pub(crate) fn criterion_mean(path: &Path) -> Result<[f64; 3]> {
    #[derive(serde::Deserialize)]
    struct Confidence {
        lower_bound: f64,
        upper_bound: f64,
    }
    #[derive(serde::Deserialize)]
    struct Mean {
        point_estimate: f64,
        confidence_interval: Confidence,
    }
    #[derive(serde::Deserialize)]
    struct Estimates {
        mean: Mean,
    }
    let data: Estimates = read_json(path)?;
    let mean = data.mean;
    let values = [
        mean.point_estimate,
        mean.confidence_interval.lower_bound,
        mean.confidence_interval.upper_bound,
    ];
    if values.iter().any(|v| !v.is_finite() || *v <= 0.0)
        || values[1] > values[0]
        || values[2] < values[0]
    {
        return Err(format!(
            "{}: invalid positive ordered confidence interval",
            path.display()
        )
        .into());
    }
    Ok(values)
}

// serde_json::Value normally accepts duplicate object keys, discarding earlier
// values. Evidence audits must report ambiguity instead of silently rewriting it.
pub(crate) struct StrictJson(pub(crate) Value);
impl<'de> Deserialize<'de> for StrictJson {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct JsonVisitor;
        impl<'de> Visitor<'de> for JsonVisitor {
            type Value = StrictJson;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("unambiguous JSON")
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                v: bool,
            ) -> std::result::Result<Self::Value, E> {
                Ok(StrictJson(v.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<Self::Value, E> {
                Ok(StrictJson(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Self::Value, E> {
                Ok(StrictJson(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| StrictJson(Value::Number(n)))
                    .ok_or_else(|| E::custom("non-finite number"))
            }
            fn visit_str<E: serde::de::Error>(
                self,
                v: &str,
            ) -> std::result::Result<Self::Value, E> {
                Ok(StrictJson(v.into()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(StrictJson(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(StrictJson(value)) = seq.next_element()? {
                    values.push(value);
                }
                Ok(StrictJson(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate object key: {key}"
                        )));
                    }
                    let StrictJson(value) = map.next_value()?;
                    values.insert(key, value);
                }
                Ok(StrictJson(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(JsonVisitor)
    }
}
