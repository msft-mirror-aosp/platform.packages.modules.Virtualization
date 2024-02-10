// Copyright 2024 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Implements converting file system to FDT blob

use anyhow::{anyhow, Context, Result};
use libfdt::Fdt;
use std::ffi::{CStr, CString};
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// Trait for Fdt's file system support
pub trait FsFdt<'a> {
    /// Creates a Fdt from /proc/device-tree style directory by wrapping a mutable slice
    fn from_fs(fs_path: &Path, fdt_buffer: &'a mut [u8]) -> Result<&'a mut Self>;

    /// Appends a FDT from /proc/device-tree style directory at the given node path
    fn append(&mut self, fdt_node_path: &CStr, fs_path: &Path) -> Result<()>;
}

impl<'a> FsFdt<'a> for Fdt {
    fn from_fs(fs_path: &Path, fdt_buffer: &'a mut [u8]) -> Result<&'a mut Fdt> {
        let fdt = Fdt::create_empty_tree(fdt_buffer)
            .map_err(|e| anyhow!("Failed to create FDT, {e:?}"))?;

        fdt.append(&CString::new("").unwrap(), fs_path)?;

        Ok(fdt)
    }

    fn append(&mut self, fdt_node_path: &CStr, fs_path: &Path) -> Result<()> {
        // Recursively traverse fs_path with DFS algorithm.
        let mut stack = vec![fs_path.to_path_buf()];
        while let Some(dir_path) = stack.pop() {
            let relative_path = dir_path
                .strip_prefix(fs_path)
                .context("Internal error. Path does not have expected prefix")?
                .as_os_str();
            let fdt_path = CString::from_vec_with_nul(
                [fdt_node_path.to_bytes(), b"/", relative_path.as_bytes(), b"\0"].concat(),
            )
            .context("Internal error. Path is not a valid Fdt path")?;

            let mut node = self
                .node_mut(&fdt_path)
                .map_err(|e| anyhow!("Failed to write FDT, {e:?}"))?
                .ok_or_else(|| anyhow!("Failed to find {fdt_node_path:?} in FDT"))?;

            let mut subnode_names = vec![];
            let entries =
                fs::read_dir(&dir_path).with_context(|| format!("Failed to read {dir_path:?}"))?;
            for entry in entries {
                let entry =
                    entry.with_context(|| format!("Failed to get an entry in {dir_path:?}"))?;
                let entry_type =
                    entry.file_type().with_context(|| "Unsupported entry type, {entry:?}")?;
                let entry_name = entry.file_name(); // binding to keep name below.
                if !entry_name.is_ascii() {
                    return Err(anyhow!("Unsupported entry name for FDT, {entry:?}"));
                }
                // Safe to unwrap because validated as an ascii string above.
                let name = CString::new(entry_name.as_bytes()).unwrap();
                if entry_type.is_dir() {
                    stack.push(entry.path());
                    subnode_names.push(name);
                } else if entry_type.is_file() {
                    let value = fs::read(&entry.path())?;

                    node.setprop(&name, &value)
                        .map_err(|e| anyhow!("Failed to set FDT property, {e:?}"))?;
                } else {
                    return Err(anyhow!(
                        "Failed to handle {entry:?}. FDT only uses file or directory"
                    ));
                }
            }
            // Note: sort() is necessary to prevent FdtError::Exists from add_subnodes().
            // FDT library may omit address in node name when comparing their name, so sort to add
            // node without address first.
            subnode_names.sort();
            let subnode_names_c_str: Vec<_> = subnode_names.iter().map(|x| x.as_c_str()).collect();
            node.add_subnodes(&subnode_names_c_str)
                .map_err(|e| anyhow!("Failed to add node, {e:?}"))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::Write;
    use std::process::Command;
    use tempfile::NamedTempFile;

    const TEST_FS_FDT_ROOT_PATH: &str = "testdata/fs";
    const BUF_SIZE_MAX: usize = 1024;

    fn dts_from_fs(path: &Path) -> String {
        let path = path.to_str().unwrap();
        let res = Command::new("./dtc_static")
            .args(["-f", "-s", "-I", "fs", "-O", "dts", path])
            .output()
            .unwrap();
        assert!(res.status.success(), "{res:?}");
        String::from_utf8(res.stdout).unwrap()
    }

    fn dts_from_dtb(path: &Path) -> String {
        let path = path.to_str().unwrap();
        let res = Command::new("./dtc_static")
            .args(["-f", "-s", "-I", "dtb", "-O", "dts", path])
            .output()
            .unwrap();
        assert!(res.status.success(), "{res:?}");
        String::from_utf8(res.stdout).unwrap()
    }

    fn to_temp_file(fdt: &Fdt) -> Result<NamedTempFile> {
        let mut file = NamedTempFile::new()?;
        file.as_file_mut().write_all(fdt.as_slice())?;
        file.as_file_mut().sync_all()?;

        Ok(file)
    }

    #[test]
    fn test_from_fs() {
        let fs_path = Path::new(TEST_FS_FDT_ROOT_PATH);

        let mut data = vec![0_u8; BUF_SIZE_MAX];
        let fdt = Fdt::from_fs(fs_path, &mut data).unwrap();
        let file = to_temp_file(fdt).unwrap();

        let expected = dts_from_fs(fs_path);
        let actual = dts_from_dtb(file.path());

        assert_eq!(&expected, &actual);
    }
}