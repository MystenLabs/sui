// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0
//
// Copied and adapted from
// <https://github.com/rust-lang/cargo/tree/master/crates/cargo-test-support/src> at SHA
// 4ac865d3d7b62281ad4dcb92406c816b6f1aeceb

pub mod git;
mod paths;

pub mod graph_builder;

use indoc::formatdoc;
use paths::PathExt;
use paths::root;
use std::env;
use std::fmt::Write;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

#[macro_export]
macro_rules! t {
    ($e:expr) => {
        match $e {
            Ok(e) => e,
            Err(e) => panic_error(&format!("failed running {}", stringify!($e)), e),
        }
    };
}

/// `panic!`, reporting the specified error , see also [`t!`]
#[track_caller]
pub fn panic_error(what: &str, err: impl Into<anyhow::Error>) -> ! {
    let err = err.into();
    pe(what, err);
    #[track_caller]
    fn pe(what: &str, err: anyhow::Error) -> ! {
        let mut result = format!("{}\nerror: {}", what, err);
        for cause in err.chain().skip(1) {
            let _ = writeln!(result, "\nCaused by:");
            let _ = write!(result, "{}", cause);
        }
        panic!("\n{}", result);
    }
}
#[derive(PartialEq, Clone)]
struct FileBuilder {
    path: PathBuf,
    body: String,
    executable: bool,
}

impl FileBuilder {
    pub fn new(path: PathBuf, body: &str, executable: bool) -> FileBuilder {
        FileBuilder {
            path,
            body: body.to_string(),
            executable,
        }
    }

    fn mk(&mut self) {
        if self.executable {
            let mut path = self.path.clone().into_os_string();
            write!(path, "{}", env::consts::EXE_SUFFIX).unwrap();
            self.path = path.into();
        }

        self.dirname().mkdir_p();
        fs::write(&self.path, &self.body)
            .unwrap_or_else(|e| panic!("could not create file {}: {}", self.path.display(), e));

        #[cfg(unix)]
        if self.executable {
            use std::os::unix::fs::PermissionsExt;

            let mut perms = fs::metadata(&self.path).unwrap().permissions();
            let mode = perms.mode();
            perms.set_mode(mode | 0o111);
            fs::set_permissions(&self.path, perms).unwrap();
        }
    }

    fn dirname(&self) -> &Path {
        self.path.parent().unwrap()
    }
}

/// A directory to run tests against.
///
/// See [`ProjectBuilder`] to get started.
#[derive(Debug)]
pub struct Project {
    root: PathBuf,
}

/// Create a project to run tests against but you need to add a manifest either via basic_manifest
/// or through your own content.
///
/// To get started, see:
/// - [`project`]
#[must_use]
pub struct ProjectBuilder {
    root: Project,
    files: Vec<FileBuilder>,
}

impl ProjectBuilder {
    /// Root of the project
    pub fn root(&self) -> PathBuf {
        self.root.root()
    }

    /// Create project in `root`
    pub fn new(root: PathBuf) -> ProjectBuilder {
        ProjectBuilder {
            root: Project { root },
            files: vec![],
        }
    }

    /// Create project, relative to [`paths::root`]
    pub fn at<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.root = Project {
            root: root().join(path),
        };
        self
    }

    /// Adds a file to the project.
    pub fn file<B: AsRef<Path>>(mut self, path: B, body: &str) -> Self {
        self._file(path.as_ref(), body, false);
        self
    }

    /// Adds an executable file to the project.
    pub fn executable<B: AsRef<Path>>(mut self, path: B, body: &str) -> Self {
        self._file(path.as_ref(), body, true);
        self
    }

    fn _file(&mut self, path: &Path, body: &str, executable: bool) {
        self.files.push(FileBuilder::new(
            self.root.root().join(path),
            body,
            executable,
        ));
    }

    /// Creates the project.
    pub fn build(mut self) -> Project {
        // First, clean the directory if it already exists
        self.rm_root();

        // Create the empty directory
        self.root.root().mkdir_p();

        for file in self.files.iter_mut() {
            file.mk();
        }

        let ProjectBuilder { root, .. } = self;
        root
    }

    fn rm_root(&self) {
        self.root.root().rm_rf()
    }
}

impl Project {
    /// Root of the project
    pub fn root(&self) -> PathBuf {
        self.root.clone()
    }

    /// Root of the project as a string. This will panic if root does not exist.
    pub fn root_path_str(&self) -> &str {
        self.root.to_str().unwrap()
    }

    /// Overwrite a file with new content
    pub fn change_file(&self, path: impl AsRef<Path>, body: &str) {
        FileBuilder::new(self.root().join(path), body, false).mk()
    }

    pub fn extend_file(&self, path: impl AsRef<Path>, body: &str) {
        let full = self.root().join(path.as_ref());
        let mut contents = fs::read_to_string(&full)
            .unwrap_or_else(|e| panic!("could not read file {}: {}", full.display(), e));
        contents.push_str(body);
        fs::write(&full, contents)
            .unwrap_or_else(|e| panic!("could not write file {}: {}", full.display(), e));
    }

    /// Returns the contents of `Move.lock`.
    pub fn read_lockfile(&self) -> String {
        self.read_file("Move.lock")
    }

    /// Returns the contents of a path in the project root
    pub fn read_file(&self, path: impl AsRef<Path>) -> String {
        let full = self.root().join(path);
        fs::read_to_string(&full)
            .unwrap_or_else(|e| panic!("could not read file {}: {}", full.display(), e))
    }

    /// Modifies `Move.toml` to remove all commented lines.
    pub fn uncomment_root_manifest(&self) {
        let contents = self.read_file("Move.toml").replace("#", "");
        fs::write(self.root().join("Move.toml"), contents).unwrap();
    }
}

/// Generates a project layout, see [`ProjectBuilder`]
pub fn project() -> ProjectBuilder {
    ProjectBuilder::new(root().join("foo"))
}

/// Generate a basic `Move.toml` content
pub fn basic_manifest(name: &str, version: &str) -> String {
    formatdoc!(
        r#"
        [package]
        name = "{}"
        version = "{}"
        authors = []
        edition = "2024"

        [environments]
        mainnet = "35834a8a"
        testnet = "4c78adac"
    "#,
        name,
        version,
    )
}

/// Generate a basic manifest with specific environment info
pub fn basic_manifest_with_env(name: &str, version: &str, env: &str, chain_id: &str) -> String {
    formatdoc!(
        r#"
        [package]
        name = "{}"
        version = "{}"
        authors = []
        edition = "2024"

        [environments]
        {} = "{}"
    "#,
        name,
        version,
        env,
        chain_id
    )
}
