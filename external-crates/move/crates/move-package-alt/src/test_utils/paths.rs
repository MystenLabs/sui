//! Access common paths and manipulate the filesystem

use std::env;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::t;
use crate::test_utils::panic_error;
use std::os::unix::fs::PermissionsExt;

/// Path to the test's filesystem scratchpad
pub fn root() -> PathBuf {
    let tempdir = tempfile::tempdir().unwrap();
    tempdir.path().to_path_buf()
}

/// Path to the current test's `$HOME`
///
/// ex: `$CARGO_TARGET_TMPDIR/cit/t0/home`
pub fn home() -> PathBuf {
    let mut path = root();
    path.push("home");
    path.mkdir_p();
    path
}

/// Path to the current test's `$CARGO_HOME`
///
/// ex: `$CARGO_TARGET_TMPDIR/cit/t0/home/.cargo`
pub fn cargo_home() -> PathBuf {
    home().join(".cargo")
}

/// Common path and file operations
pub trait CargoPathExt {
    fn to_url(&self) -> url::Url;

    fn rm_rf(&self);
    fn mkdir_p(&self);

    /// Returns a list of all files and directories underneath the given
    /// directory, recursively, including the starting path.
    fn ls_r(&self) -> Vec<PathBuf>;
}

impl CargoPathExt for Path {
    fn to_url(&self) -> url::Url {
        url::Url::from_file_path(self).ok().unwrap()
    }

    fn rm_rf(&self) {
        let meta = match self.symlink_metadata() {
            Ok(meta) => meta,
            Err(e) => {
                if e.kind() == ErrorKind::NotFound {
                    return;
                }
                panic!("failed to remove {:?}, could not read: {:?}", self, e);
            }
        };
        // There is a race condition between fetching the metadata and
        // actually performing the removal, but we don't care all that much
        // for our tests.
        if meta.is_dir() {
            if let Err(e) = fs::remove_dir_all(self) {
                panic!("failed to remove {:?}: {:?}", self, e)
            }
        } else if let Err(e) = fs::remove_file(self) {
            panic!("failed to remove {:?}: {:?}", self, e)
        }
    }

    fn mkdir_p(&self) {
        fs::create_dir_all(self)
            .unwrap_or_else(|e| panic!("failed to mkdir_p {}: {}", self.display(), e))
    }

    fn ls_r(&self) -> Vec<PathBuf> {
        walkdir::WalkDir::new(self)
            .sort_by_file_name()
            .into_iter()
            .filter_map(|e| e.map(|e| e.path().to_owned()).ok())
            .collect()
    }
}

impl CargoPathExt for PathBuf {
    fn to_url(&self) -> url::Url {
        self.as_path().to_url()
    }

    fn rm_rf(&self) {
        self.as_path().rm_rf()
    }
    fn mkdir_p(&self) {
        self.as_path().mkdir_p()
    }

    fn ls_r(&self) -> Vec<PathBuf> {
        self.as_path().ls_r()
    }
}

fn do_op<F>(path: &Path, desc: &str, mut f: F)
where
    F: FnMut(&Path) -> io::Result<()>,
{
    match f(path) {
        Ok(()) => {}
        Err(ref e) if e.kind() == ErrorKind::PermissionDenied => {
            let mut p = t!(path.metadata()).permissions();
            p.set_readonly(false);
            t!(fs::set_permissions(path, p));

            // Unix also requires the parent to not be readonly for example when
            // removing files
            let parent = path.parent().unwrap();
            let mut p = t!(parent.metadata()).permissions();
            p.set_readonly(false);
            t!(fs::set_permissions(parent, p));

            f(path).unwrap_or_else(|e| {
                panic!("failed to {} {}: {}", desc, path.display(), e);
            })
        }
        Err(e) => {
            panic!("failed to {} {}: {}", desc, path.display(), e);
        }
    }
}
