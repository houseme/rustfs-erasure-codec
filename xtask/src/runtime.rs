//! Canonical, guest-reported CPU identity; no claim of physical-host exclusivity.
use crate::Result;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

const ARM_FIELDS: [&str; 5] = [
    "CPU implementer",
    "CPU architecture",
    "CPU variant",
    "CPU part",
    "CPU revision",
];

fn ascii_json(text: &str) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity(text.len());
    for character in text.chars() {
        if character <= '~' {
            result.push(character);
        } else {
            for unit in character.encode_utf16(&mut [0; 2]) {
                write!(result, "\\u{unit:04x}").expect("writing into a String");
            }
        }
    }
    result
}

fn number(text: &str, decimal: bool) -> Result<u64> {
    let text = text.strip_prefix('+').unwrap_or(text);
    let (digits, radix) = if decimal {
        (text, 10)
    } else if let Some(value) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        (value, 16)
    } else if let Some(value) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
        (value, 8)
    } else if let Some(value) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
        (value, 2)
    } else {
        (text, 10)
    };
    u64::from_str_radix(digits, radix).map_err(|_| "invalid ARM CPU identity value".into())
}

pub(crate) fn cpu_identity(text: &str, machine: &str, reserved: &[u64]) -> Result<String> {
    if let Some(model) = text
        .lines()
        .filter(|line| line.starts_with("model name"))
        .filter_map(|line| line.split_once(':').map(|(_, value)| value.trim()))
        .next()
        .filter(|value| !value.is_empty())
    {
        return Ok(model.to_owned());
    }
    if !["aarch64", "arm64"].contains(&machine.to_ascii_lowercase().as_str()) {
        return Err("CPU model must be available for release evidence".into());
    }
    let mut processors = BTreeMap::new();
    let mut fields = BTreeMap::new();
    let mut finish = |fields: &mut BTreeMap<String, String>| -> Result<()> {
        if let Some(cpu) = fields.get("processor") {
            if cpu.is_empty() || !cpu.bytes().all(|c| c.is_ascii_digit()) {
                return Err("invalid ARM CPU index".into());
            }
            let cpu: u64 = cpu.parse().map_err(|_| "invalid ARM CPU index")?;
            if processors.insert(cpu, std::mem::take(fields)).is_some() {
                return Err("duplicate ARM CPU index".into());
            }
        } else {
            fields.clear();
        }
        Ok(())
    };
    for line in text.lines().chain(std::iter::once("")) {
        if line.trim().is_empty() {
            finish(&mut fields)?;
            continue;
        }
        if let Some((key, value)) = line.split_once(':')
            && fields
                .insert(key.trim().to_owned(), value.trim().to_owned())
                .is_some()
        {
            return Err("duplicate ARM CPU identity field".into());
        }
    }
    let mut cpus = reserved.to_vec();
    cpus.sort_unstable();
    let mut identities = Vec::with_capacity(cpus.len());
    for cpu in cpus {
        let fields = processors
            .get(&cpu)
            .ok_or("complete ARM identity required for every reserved CPU")?;
        for field in ARM_FIELDS.iter().copied().chain(["Features"]) {
            if fields.get(field).is_none_or(|value| value.is_empty()) {
                return Err("complete ARM identity required for every reserved CPU".into());
            }
        }
        let mut identity = Map::new();
        identity.insert("cpu".into(), cpu.into());
        for field in ARM_FIELDS {
            identity.insert(
                field.into(),
                number(&fields[field], field == "CPU architecture")?.into(),
            );
        }
        let features: BTreeSet<_> = fields["Features"].split_whitespace().collect();
        identity.insert(
            "Features".into(),
            features.into_iter().collect::<Vec<_>>().into(),
        );
        identities.push(Value::Object(identity));
    }
    if identities.is_empty() {
        return Err("reserved ARM CPUs are required".into());
    }
    Ok(format!(
        "aarch64 guest CPU identity: {}",
        ascii_json(&serde_json::to_string(&identities)?)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(cpu: u64, part: &str) -> String {
        format!(
            "processor: {cpu}\nCPU implementer: 0x61\nCPU architecture: 8\nCPU variant: 0x0\nCPU part: {part}\nCPU revision: 0\nFeatures: fp asimd aes\n"
        )
    }
    #[test]
    fn named_model_is_retained_and_unknown_architectures_cannot_invent_a_name() {
        assert_eq!(
            cpu_identity(
                "processor: 0\nmodel name: fixture x86 CPU\n",
                "x86_64",
                &[0]
            )
            .unwrap(),
            "fixture x86 CPU"
        );
        assert!(cpu_identity(&record(0, "0x000"), "unknown", &[0]).is_err());
    }
    #[test]
    fn arm_identity_is_canonical_and_bound_to_reserved_processors() {
        let first = record(0, "0x000");
        let second = record(1, "0x001");
        let before = cpu_identity(&format!("{first}\n{second}"), "aarch64", &[0, 1]).unwrap();
        let reordered = format!(
            "{}\n{}",
            second.replace("fp asimd aes", "aes fp asimd"),
            first
        );
        assert_eq!(cpu_identity(&reordered, "ARM64", &[1, 0]).unwrap(), before);
        assert_ne!(
            cpu_identity(&format!("{first}\n{second}"), "aarch64", &[0]).unwrap(),
            before
        );
        let changed = second.replace("0x001", "0x002");
        assert_ne!(
            cpu_identity(&format!("{first}\n{changed}"), "aarch64", &[0, 1]).unwrap(),
            before
        );
        assert_eq!(
            cpu_identity(&format!("{first}\n{changed}"), "aarch64", &[0]).unwrap(),
            cpu_identity(&first, "aarch64", &[0]).unwrap()
        );
        let unicode = first.replace("fp asimd aes", "fp λ 😀");
        let identity = cpu_identity(&unicode, "aarch64", &[0]).unwrap();
        assert!(identity.contains("\\u03bb"));
        assert!(identity.contains("\\ud83d\\ude00"));
    }
    #[test]
    fn incomplete_duplicate_or_invalid_processor_fields_are_refused() {
        let original = record(0, "0x000");
        for field in [
            "processor",
            "CPU implementer",
            "CPU architecture",
            "CPU variant",
            "CPU part",
            "CPU revision",
            "Features",
        ] {
            let missing = original
                .lines()
                .filter(|line| !line.starts_with(&format!("{field}:")))
                .collect::<Vec<_>>()
                .join("\n");
            assert!(cpu_identity(&missing, "aarch64", &[0]).is_err(), "{field}");
        }
        for field in [
            "CPU implementer",
            "CPU architecture",
            "CPU variant",
            "CPU part",
            "CPU revision",
        ] {
            let malformed = original
                .lines()
                .map(|line| {
                    if line.starts_with(&format!("{field}:")) {
                        format!("{field}: invalid")
                    } else {
                        line.into()
                    }
                })
                .collect::<Vec<String>>()
                .join("\n");
            assert!(
                cpu_identity(&malformed, "aarch64", &[0]).is_err(),
                "{field}"
            );
        }
        assert!(cpu_identity(&format!("{original}\n{original}"), "aarch64", &[0]).is_err());
        assert!(cpu_identity(&original, "aarch64", &[0, 1]).is_err());
        assert!(cpu_identity(&original, "aarch64", &[]).is_err());
        assert!(cpu_identity(&(original + "CPU part: 0x001\n"), "aarch64", &[0]).is_err());
    }
}
