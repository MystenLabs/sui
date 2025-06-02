use std::{
    cell::RefCell,
    io,
    path::{Path, PathBuf},
};

use append_only_vec::AppendOnlyVec;
use tracing::debug;

use super::FileHandle;

thread_local! {
    /// The ID of the file currently being parsed
    static PARSING_FILE: RefCell<Option<FileHandle>> = const { RefCell::new(None) };
}

pub struct TheFile;

struct Guard;

/// This is a little helper for keeping track of what file a deserialized value comes from.
/// By deserializing in a with_file context, deserializers can use [TheFile::handle] or
/// [TheFile::parent_dir] examine the context.
///
/// For example, you could deserialize a struct containing a file handle as follows:
/// ```ignore
///     #[derive(Deserialize)]
///     struct S {
///         #[serde(skip, default = "TheFile::handle")]
///         containing_file: FileHandle,
///
///         #[serde(skip, default = "TheFile::parent_dir")]
///         containing_dir: PathBuf,
///     }
///
///     fn main() {
///         let (s, handle) = TheFile::with_file("foo/bar.txt", toml_edit::de::from_str::<S>)?;
///         assert_eq!(handle.path(), "foo/bar.txt");
///         assert_eq!(s.containing_file, handle);
///         assert_eq!(s.containing_dir, "foo/");
///     }
/// ```
impl TheFile {
    /// The handle of the file that is currently being processed by this thread
    pub fn handle() -> FileHandle {
        PARSING_FILE.with_borrow(|f| {
            f.expect("current_file can only be called in the context of with_file")
        })
    }

    /// Return the parent directory of current_file, panicking if there is no current_file
    pub fn parent_dir() -> PathBuf {
        Self::handle()
            .path()
            .parent()
            .expect("PARSING_FILE should be a file, and therefore should have a parent directory")
            .to_path_buf()
    }

    /// Read `file` into the cache and run `f` with `current_file` set to the handle. Panics if
    /// `current_dir` is already set. Unsets `current_file` before returning. Returns the generated
    /// file handle and the result of `f`
    pub fn with_file<R, F: FnOnce(&str) -> R>(
        file: impl AsRef<Path>,
        f: F,
    ) -> io::Result<(R, FileHandle)> {
        let buf = file.as_ref().to_path_buf();
        let file_id = FileHandle::new(buf)?;

        let guard = Guard::new(file_id);
        let result: R = f(file_id.source());

        Ok((result, file_id))
    }

    /// Run `f` with `current_file` set to `file`
    pub async fn with_existing<R, F: AsyncFnOnce() -> R>(file: FileHandle, f: F) -> R {
        let guard = Guard::new(file);
        f().await
    }
}

impl Guard {
    fn new(file: FileHandle) -> Self {
        let result = Self {};
        PARSING_FILE.with_borrow(|old| {
            if let Some(old) = old {
                panic!(
                    "Cannot call parse_toml_file from within a deserializer; replacing {old:?} with {file:?}",
                );
            }
        });
        PARSING_FILE.set(Some(file));
        result
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        PARSING_FILE.set(None)
    }
}
