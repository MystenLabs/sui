// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::bail;
use std::{cmp::Ordering, path::Path};
use tempfile::tempdir;
use walkdir::{DirEntry, WalkDir};

#[test]
fn check_that_docs_are_updated() {
    let temp_dir = tempdir().unwrap();

    crate::build_stdlib_doc(&temp_dir.path().to_string_lossy());

    let res = check_dirs_not_diff(&temp_dir, &crate::move_stdlib_docs_full_path());
    assert!(
        res.is_ok(),
        "Generated docs differ from the ones checked in. {}",
        res.unwrap_err()
    )
}

#[test]
fn check_that_the_errmap_is_updated() {
    let temp_file = tempfile::NamedTempFile::new().unwrap();

    crate::build_error_code_map(&temp_file.path().to_string_lossy());

    assert!(
        file_diff::diff(
            &temp_file.path().to_string_lossy(),
            &crate::move_stdlib_errmap_full_path()
        ),
        "Generated errmap differ from the one checked in"
    );
}

fn check_dirs_not_diff<A: AsRef<Path>, B: AsRef<Path>>(
    actual: A,
    expected: B,
) -> anyhow::Result<()> {
    let mut act_walker = sorted_walk_dir(actual)?;
    let mut exp_walker = sorted_walk_dir(expected)?;

    for (a, b) in (&mut act_walker).zip(&mut exp_walker) {
        let a = a?;
        let b = b?;

        if a.depth() != b.depth() {
            bail!(
                "Mismatched depth for {} and {}",
                display_dir_entry(a),
                display_dir_entry(b),
            )
        }
        if a.file_type() != b.file_type() {
            bail!(
                "Mismatched file type for {} and {}",
                display_dir_entry(a),
                display_dir_entry(b),
            )
        }
        if a.file_name() != b.file_name() {
            bail!(
                "Mismatched file name for {} and {}",
                display_dir_entry(a),
                display_dir_entry(b),
            )
        }
        if a.file_type().is_file() && std::fs::read(a.path())? != std::fs::read(b.path())? {
            bail!("{} needs to be updated", display_dir_entry(a))
        }
    }

    if let Some(a) = act_walker.next() {
        bail!(
            "Unexpected dir entry: {}. Not found in expected",
            display_dir_entry(a?)
        )
    }
    if let Some(b) = exp_walker.next() {
        bail!(
            "Expected dir entry: {}. Not found in actual",
            display_dir_entry(b?)
        )
    }
    Ok(())
}

fn sorted_walk_dir<P: AsRef<Path>>(path: P) -> Result<walkdir::IntoIter, std::io::Error> {
    fn compare_by_file_name(a: &DirEntry, b: &DirEntry) -> Ordering {
        a.file_name().cmp(b.file_name())
    }

    let mut walkdir = WalkDir::new(path).sort_by(compare_by_file_name).into_iter();
    match walkdir.next() {
        Some(Err(e)) => Err(e.into()),
        _ => Ok(walkdir),
    }
}

fn display_dir_entry(d: walkdir::DirEntry) -> String {
    d.into_path().to_string_lossy().to_string()
}
