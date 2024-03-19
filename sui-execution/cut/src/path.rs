// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::{bail, Result};
use thiserror::Error;

#[derive(Error, Debug)]
pub(crate) enum Error {
    #[error("Path attempts to access parent of root directory: {}", .0.display())]
    ParentOfRoot(PathBuf),

    #[error("Unexpected symlink: {}", .0.display())]
    Symlink(PathBuf),
}

/// Normalize the representation of `path` by eliminating redundant `.` components and applying `..`
/// components.  Does not access the filesystem (e.g. to resolve symlinks or test for file
/// existence), unlike `std::fs::canonicalize`.
///
/// Fails if the normalized path attempts to access the parent of a root directory or volume
/// prefix.  Returns the normalized path on success.
pub(crate) fn normalize_path<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
    use std::path::Component as C;

    let mut stack = vec![];
    for component in path.as_ref().components() {
        match component {
            // Components that contribute to the path as-is.
            verbatim @ (C::Prefix(_) | C::RootDir | C::Normal(_)) => stack.push(verbatim),

            // Equivalent of a `.` path component -- can be ignored.
            C::CurDir => { /* nop */ }

            // Going up in the directory hierarchy, which may fail if that's not possible.
            C::ParentDir => match stack.last() {
                None | Some(C::ParentDir) => {
                    stack.push(C::ParentDir);
                }

                Some(C::Normal(_)) => {
                    stack.pop();
                }

                Some(C::CurDir) => {
                    unreachable!("Component::CurDir never added to the stack");
                }

                Some(C::RootDir | C::Prefix(_)) => {
                    bail!(Error::ParentOfRoot(path.as_ref().to_path_buf()))
                }
            },
        }
    }

    Ok(stack.iter().collect())
}

/// Return the path to `dst` relative to `src`.  If `src` is a file, the path is relative to the
/// directory that contains it, while if it is a directory, the path is relative to it.  Returns
/// an error if either `src` or `dst` do not exist.
pub(crate) fn path_relative_to<P, Q>(src: P, dst: Q) -> io::Result<PathBuf>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    use std::path::Component as C;

    let mut src = fs::canonicalize(src)?;
    let dst = fs::canonicalize(dst)?;

    if src.is_file() {
        src.pop();
    }

    let mut s_comps = src.components().peekable();
    let mut d_comps = dst.components().peekable();

    // (1). Strip matching prefix
    loop {
        let Some(s_comp) = s_comps.peek() else { break };
        let Some(d_comp) = d_comps.peek() else { break };
        if s_comp != d_comp {
            break;
        }

        s_comps.next();
        d_comps.next();
    }

    // (2) Push parent directory components (moving out of directories in `base`)
    let mut stack = vec![];
    for _ in s_comps {
        stack.push(C::ParentDir)
    }

    // (3) Push extension directory components (moving into directories unique to `ext`)
    for comp in d_comps {
        stack.push(comp)
    }

    // (4) Check for base == ext case
    if stack.is_empty() {
        stack.push(C::CurDir)
    }

    Ok(stack.into_iter().collect())
}

/// Returns the shortest prefix of `path` that doesn't exist, or `None` if `path` already exists.
pub(crate) fn shortest_new_prefix(path: impl AsRef<Path>) -> Option<PathBuf> {
    if path.as_ref().exists() {
        return None;
    }

    let mut path = path.as_ref().to_owned();
    let mut parent = path.clone();
    parent.pop();

    // Invariant: parent == { path.pop(); path }
    //         && !path.exists()
    while !parent.exists() {
        parent.pop();
        path.pop();
    }

    Some(path)
}

/// Recursively copy the contents of `src` to `dst`.  Fails if `src` transitively contains a
/// symlink.  Only copies paths that pass the `keep` predicate.
pub(crate) fn deep_copy<P, Q, K>(src: P, dst: Q, keep: &mut K) -> Result<()>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
    K: FnMut(&Path) -> bool,
{
    let src = src.as_ref();
    let dst = dst.as_ref();

    if !keep(src) {
        return Ok(());
    }

    if src.is_file() {
        fs::create_dir_all(dst.parent().expect("files have parents"))?;
        fs::copy(src, dst)?;
        return Ok(());
    }

    if src.is_symlink() {
        bail!(Error::Symlink(src.to_path_buf()));
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        deep_copy(
            src.join(entry.file_name()),
            dst.join(entry.file_name()),
            keep,
        )?
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use expect_test::expect;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_normalize_path_identity() {
        assert_eq!(normalize_path("/a/b").unwrap(), PathBuf::from("/a/b"));
        assert_eq!(normalize_path("/").unwrap(), PathBuf::from("/"));
        assert_eq!(normalize_path("a/b").unwrap(), PathBuf::from("a/b"));
    }

    #[test]
    fn test_normalize_path_absolute() {
        assert_eq!(normalize_path("/a/./b").unwrap(), PathBuf::from("/a/b"));
        assert_eq!(normalize_path("/a/../b").unwrap(), PathBuf::from("/b"));
    }

    #[test]
    fn test_normalize_path_relative() {
        assert_eq!(normalize_path("a/./b").unwrap(), PathBuf::from("a/b"));
        assert_eq!(normalize_path("a/../b").unwrap(), PathBuf::from("b"));
        assert_eq!(normalize_path("a/../../b").unwrap(), PathBuf::from("../b"));
    }

    #[test]
    fn test_normalize_path_error() {
        expect!["Path attempts to access parent of root directory: /a/../.."]
            .assert_eq(&format!("{}", normalize_path("/a/../..").unwrap_err()))
    }

    #[test]
    fn test_path_relative_to_equal() {
        let cut = env!("CARGO_MANIFEST_DIR");
        assert_eq!(path_relative_to(cut, cut).unwrap(), PathBuf::from("."));
    }

    #[test]
    fn test_path_relative_to_file() {
        let cut = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let toml = cut.join("Cargo.toml");
        let src = cut.join("src");

        // Paths relative to files will be relative to their directory, whereas paths relative to
        // directories will not.
        assert_eq!(path_relative_to(&toml, &src).unwrap(), PathBuf::from("src"));
        assert_eq!(
            path_relative_to(&src, &toml).unwrap(),
            PathBuf::from("../Cargo.toml")
        );
    }

    #[test]
    fn test_path_relative_to_related() {
        let cut = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let src = cut.join("src");
        let repo_root = cut.join("../..");

        // Paths relative to files will be relative to their directory, whereas paths relative to
        // directories will not.
        assert_eq!(path_relative_to(&cut, &src).unwrap(), PathBuf::from("src"));
        assert_eq!(path_relative_to(&src, &cut).unwrap(), PathBuf::from(".."));

        assert_eq!(
            path_relative_to(&repo_root, &src).unwrap(),
            PathBuf::from("sui-execution/cut/src"),
        );

        assert_eq!(
            path_relative_to(&src, &repo_root).unwrap(),
            PathBuf::from("../../.."),
        );
    }

    #[test]
    fn test_path_relative_to_unrelated() {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let sui_adapter = repo_root.join("sui-execution/latest/sui-adapter");
        let vm_runtime = repo_root.join("external-crates/move/crates/move-vm-runtime");

        assert_eq!(
            path_relative_to(sui_adapter, vm_runtime).unwrap(),
            PathBuf::from("../../../external-crates/move/crates/move-vm-runtime"),
        );
    }

    #[test]
    fn test_path_relative_to_nonexistent() {
        let tmp = tempdir().unwrap();
        let i_dont_exist = tmp.path().join("i_dont_exist");

        expect!["No such file or directory (os error 2)"].assert_eq(&format!(
            "{}",
            path_relative_to(&i_dont_exist, &tmp).unwrap_err()
        ));

        expect!["No such file or directory (os error 2)"].assert_eq(&format!(
            "{}",
            path_relative_to(&tmp, &i_dont_exist).unwrap_err()
        ));
    }

    #[test]
    fn test_shortest_new_prefix_current() {
        let tmp = tempdir().unwrap();
        let foo = tmp.path().join("foo");
        assert_eq!(shortest_new_prefix(&foo), Some(foo));
    }

    #[test]
    fn test_shortest_new_prefix_parent() {
        let tmp = tempdir().unwrap();
        let foo = tmp.path().join("foo");
        let bar = tmp.path().join("foo/bar");
        assert_eq!(shortest_new_prefix(bar), Some(foo));
    }

    #[test]
    fn test_shortest_new_prefix_transitive() {
        let tmp = tempdir().unwrap();
        let foo = tmp.path().join("foo");
        let qux = tmp.path().join("foo/bar/baz/qux");
        assert_eq!(shortest_new_prefix(qux), Some(foo));
    }

    #[test]
    fn test_shortest_new_prefix_not_new() {
        let tmp = tempdir().unwrap();
        assert_eq!(None, shortest_new_prefix(tmp.path()));
    }

    #[test]
    fn test_deep_copy() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");

        // Set-up some things to copy:
        //
        // src/foo:         bar
        // src/baz/qux/quy: plugh
        // src/baz/quz:     xyzzy

        fs::create_dir_all(src.join("baz/qux")).unwrap();
        fs::write(src.join("foo"), "bar").unwrap();
        fs::write(src.join("baz/qux/quy"), "plugh").unwrap();
        fs::write(src.join("baz/quz"), "xyzzy").unwrap();

        let read = |path: &str| fs::read_to_string(dst.join(path)).unwrap();

        // Copy without filtering
        deep_copy(&src, dst.join("cpy-0"), &mut |_| true).unwrap();

        assert_eq!(read("cpy-0/foo"), "bar");
        assert_eq!(read("cpy-0/baz/qux/quy"), "plugh");
        assert_eq!(read("cpy-0/baz/quz"), "xyzzy");

        // Filter a file
        deep_copy(&src, dst.join("cpy-1"), &mut |p| !p.ends_with("foo")).unwrap();

        assert!(!dst.join("cpy-1/foo").exists());
        assert_eq!(read("cpy-1/baz/qux/quy"), "plugh");
        assert_eq!(read("cpy-1/baz/quz"), "xyzzy");

        // Filter a directory
        deep_copy(&src, dst.join("cpy-2"), &mut |p| !p.ends_with("baz")).unwrap();

        assert_eq!(read("cpy-2/foo"), "bar");
        assert!(!dst.join("cpy-2/baz").exists());

        // Filtering a file gets rid of its (empty) parent
        deep_copy(&src, dst.join("cpy-3"), &mut |p| !p.ends_with("quy")).unwrap();

        // Because qux is now empty, it also doesn't exist in the copy, even though we only
        // explicitly filtered `quy`.
        assert_eq!(read("cpy-3/foo"), "bar");
        assert!(!dst.join("cpy-3/baz/qux").exists());
        assert_eq!(read("cpy-3/baz/quz"), "xyzzy");
    }
}
