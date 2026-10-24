use super::{environment as env, io::Context, *};
use std::{collections::BTreeSet, fs, path::Path};
fn variable(name: &str) -> Result<String> {
    std::env::var(name).map_err(|_| format!("{name} is required").into())
}
fn ci_status(kind: &str, output: &Path) -> Result<()> {
    let mut report = match kind {
        "input" => {
            json!({"status":variable("INPUT_STATUS")?,"candidate_commit":variable("GITHUB_SHA")?,"evidence_ref":variable("EVIDENCE_REF")?,"run_id":variable("GITHUB_RUN_ID")?})
        }
        "preflight" => {
            json!({"status":variable("PREFLIGHT_STATUS")?,"profile":"release","candidate_commit":io::output(tool(),"git",&["rev-parse","HEAD"] )?,"candidate_tree":io::output(tool(),"git",&["rev-parse","HEAD^{tree}"] )?,"evidence_ref":variable("EVIDENCE_REF")?,"run_id":variable("GITHUB_RUN_ID")?,"run_attempt":variable("GITHUB_RUN_ATTEMPT")?,"ref":variable("GITHUB_REF")?,"registry_uploaded":false})
        }
        _ => return Err("unknown CI status kind".into()),
    };
    if !["success", "failure", "cancelled"]
        .contains(&report["status"].as_str().ok_or("invalid CI status")?)
    {
        report["status"] = "failure".into();
        report["error"] = "invalid CI status".into();
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    io::write_json(output, &report, false)
}
fn verification_resources(input: &Path) -> Result<()> {
    let rows: Vec<Value> = fs::read_to_string(input)?
        .lines()
        .filter(|line| line.starts_with('{'))
        .map(|line| {
            let crate::artifact::StrictJson(value) = serde_json::from_str(line)?;
            Ok(value)
        })
        .collect::<Result<_>>()?;
    let expected: BTreeSet<_> = [
        "verify.ordinary",
        "verify.caller_buffer",
        "verify.workspace",
    ]
    .into_iter()
    .flat_map(|op| {
        [(4_u64, 2_u64, 16384_u64), (10, 4, 32768)]
            .into_iter()
            .map(move |(data, parity, size)| (op.to_owned(), data, parity, size))
    })
    .collect();
    let mut actual = BTreeSet::new();
    for row in &rows {
        let op = row["operation"]
            .as_str()
            .ok_or("missing resource operation")?;
        let data = row["data_shards"].as_u64().ok_or("invalid data count")?;
        let parity = row["parity_shards"]
            .as_u64()
            .ok_or("invalid parity count")?;
        let size = row["shard_size"].as_u64().ok_or("invalid shard size")?;
        if !actual.insert((op.to_owned(), data, parity, size)) {
            return Err("duplicate resource case".into());
        }
        let ordinary = op == "verify.ordinary";
        if row["allocations"].as_u64() != Some(u64::from(ordinary))
            || row["allocated_bytes"].as_u64()
                != Some(if ordinary {
                    parity.checked_mul(size).ok_or("resource byte overflow")?
                } else {
                    0
                })
        {
            return Err("verification allocation request differs".into());
        }
    }
    if rows.len() != 6 || actual != expected {
        return Err("six complete verification resource cases are required".into());
    }
    println!("Six verification byte/corruption and allocation cases passed; no timing acceptance");
    Ok(())
}
pub(super) fn cli(_context: &Context<'_>, command: &str, args: Vec<String>) -> Result<()> {
    match command {
        "runtime-context" => {
            Options::command(command, args)?;
            println!("{}", env::runtime_context(&env::capture())?);
        }
        "lock-version" => {
            let options = Options::command(command, args)?;
            let path = options.get("--lock").unwrap_or("Cargo.lock");
            let lock: toml::Value = toml::from_str(&fs::read_to_string(path)?)?;
            let package = options.required("--package")?;
            let matches: Vec<_> = lock["package"]
                .as_array()
                .ok_or("invalid Cargo lockfile")?
                .iter()
                .filter(|p| p["name"].as_str() == Some(package))
                .collect();
            if matches.len() != 1 {
                return Err("one exact locked package version is required".into());
            }
            println!(
                "{}",
                matches[0]["version"]
                    .as_str()
                    .ok_or("missing locked version")?
            );
        }
        "check-verification-resources" => {
            let options = Options::command(command, args)?;
            verification_resources(&options.path("--input")?)?;
        }
        "write-ci-status" => {
            let options = Options::command(command, args)?;
            ci_status(options.required("--kind")?, &options.path("--output")?)?;
        }
        _ => return Err("unknown native utility".into()),
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::snapshot::tests::Directory;
    #[test]
    fn all_resource_coordinates_and_byte_counts_are_required() {
        let d = Directory::new();
        let input = d.0.join("resources.log");
        let rows:Vec<_>=["verify.ordinary","verify.caller_buffer","verify.workspace"].into_iter().flat_map(|op|[(4,2,16384),(10,4,32768)].into_iter().map(move |(data,parity,size)|json!({"operation":op,"data_shards":data,"parity_shards":parity,"shard_size":size,"allocations":u64::from(op=="verify.ordinary"),"allocated_bytes":if op=="verify.ordinary"{parity*size}else{0}}))).collect();
        let write = |rows: &[Value]| {
            fs::write(
                &input,
                rows.iter().map(|r| format!("{r}\n")).collect::<String>(),
            )
            .unwrap()
        };
        write(&rows);
        verification_resources(&input).unwrap();
        write(&rows[..5]);
        assert!(verification_resources(&input).is_err());
        let mut invalid = rows.clone();
        invalid[1]["allocated_bytes"] = 1.into();
        write(&invalid);
        assert!(verification_resources(&input).is_err());
        let mut invalid = rows.clone();
        invalid[5] = invalid[0].clone();
        write(&invalid);
        assert!(verification_resources(&input).is_err());
    }
}
