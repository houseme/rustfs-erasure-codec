use super::io::{self, Context};
use crate::Result;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
/// One transaction owns generated and patched inputs; validate the whole set before restoration.
pub(super) struct Overlay<'a, 'b> {
    root: PathBuf,
    expected: BTreeMap<PathBuf, Vec<u8>>,
    original: BTreeMap<PathBuf, Vec<u8>>,
    directory: Option<PathBuf>,
    context: &'a Context<'b>,
    active: bool,
}
impl<'a, 'b> Overlay<'a, 'b> {
    pub fn generated(
        context: &'a Context<'b>,
        root: &Path,
        files: BTreeMap<String, String>,
    ) -> Result<Self> {
        let entry = root.join("tests/rse_release_sampling.rs");
        let directory = root.join("tests/rse_release_sampling");
        if io::present(&entry) || io::present(&directory) {
            return Err("refuse to overwrite a sampling overlay".into());
        }
        if !fs::symlink_metadata(root.join("tests"))?.is_dir() {
            return Err("require a regular tests directory".into());
        }
        fs::create_dir(&directory)?;
        let mut owned = Self {
            root: root.to_owned(),
            expected: BTreeMap::new(),
            original: BTreeMap::new(),
            directory: Some(directory.clone()),
            context,
            active: true,
        };
        for (name, text) in files {
            if name != "root.rs"
                && (Path::new(&name).components().count() != 1 || !name.ends_with(".rs"))
            {
                return Err("invalid generated source name".into());
            }
            let path = if name == "root.rs" {
                entry.clone()
            } else {
                directory.join(name)
            };
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            owned.expected.insert(path, text.as_bytes().to_vec());
            file.write_all(text.as_bytes())?;
        }
        if !owned.expected.contains_key(&entry) {
            return Err("missing sampling entry point".into());
        }
        Ok(owned)
    }
    pub fn source(
        context: &'a Context<'b>,
        root: &Path,
        original: BTreeMap<String, String>,
        files: BTreeMap<String, String>,
    ) -> Result<Self> {
        let mut saved = BTreeMap::new();
        for name in original.keys() {
            let path = root.join(name);
            if !fs::symlink_metadata(&path)?.is_file() {
                return Err("require regular source-copy inputs".into());
            }
            let bytes = fs::read(&path)?;
            if String::from_utf8(bytes.clone())?
                .replace("\r\n", "\n")
                .replace('\r', "\n")
                != original[name]
            {
                return Err("source changed before observer installation".into());
            }
            saved.insert(path, bytes);
        }
        for name in files.keys() {
            let path = root.join(name);
            if !saved.contains_key(&path) && io::present(&path) {
                return Err("preserve an existing source observer".into());
            }
        }
        let mut owned = Self {
            root: root.to_owned(),
            expected: BTreeMap::new(),
            original: saved,
            directory: None,
            context,
            active: true,
        };
        for (name, text) in files {
            let path = root.join(name);
            owned
                .expected
                .insert(path.clone(), text.as_bytes().to_vec());
            if owned.original.contains_key(&path) {
                fs::write(path, text)?;
            } else {
                let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
                file.write_all(text.as_bytes())?;
            }
        }
        Ok(owned)
    }
    fn restore(&self) -> Result<()> {
        if self.context.unreaped.get() {
            return Err("unreaped worker; preserve generated inputs".into());
        }
        for (path, bytes) in &self.expected {
            if !fs::symlink_metadata(path)?.is_file() || fs::read(path)? != *bytes {
                return Err(format!(
                    "unexpected generated/source input; preserve {}",
                    self.root.display()
                )
                .into());
            }
        }
        if let Some(directory) = &self.directory {
            if !fs::symlink_metadata(directory)?.is_dir() {
                return Err("changed sampling directory; preserve it".into());
            }
            let actual: BTreeSet<_> = fs::read_dir(directory)?
                .map(|e| Ok(e?.path()))
                .collect::<Result<_>>()?;
            let expected: BTreeSet<_> = self
                .expected
                .keys()
                .filter(|p| p.parent() == Some(directory))
                .cloned()
                .collect();
            if actual != expected {
                return Err("unexpected generated input; preserve it for inspection".into());
            }
        }
        for (path, bytes) in &self.original {
            fs::write(path, bytes)?;
        }
        for path in self
            .expected
            .keys()
            .filter(|p| !self.original.contains_key(*p))
        {
            fs::remove_file(path)?;
        }
        if let Some(directory) = &self.directory {
            fs::remove_dir(directory)?;
        }
        Ok(())
    }
    pub fn finish(mut self) -> Result<()> {
        self.active = false;
        let result = self.restore();
        if result.is_err() {
            self.context.preserve_inputs.set(true);
        }
        result
    }
}
impl Drop for Overlay<'_, '_> {
    fn drop(&mut self) {
        if self.active
            && let Err(error) = self.restore()
        {
            self.context.preserve_inputs.set(true);
            eprintln!("overlay cleanup failed: {error}");
        }
    }
}

#[cfg(test)]
mod tests;
