use std::{
    cell::RefCell,
    cmp::Ordering,
    error::Error,
    fs,
    hash::{Hash, Hasher},
    ops::Range,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
};

use codespan_reporting::files::SimpleFiles;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_spanned::Spanned;

use super::{FileHandle, PackageResult};

thread_local! {
    /// The ID of the file currently being parsed
    static PARSING_FILE: RefCell<Option<FileHandle>> = const { RefCell::new(None) };
}

/// A located value contains both a file location and a span. Located values (and data structures
/// that contain them) can only be deserialized inside of [with_file]; attempting to deserialize
/// outside of `with_file` will panic
#[derive(Serialize, Debug, Clone)]
pub struct Located<T> {
    // TODO: use move-compiler::shared::files to avoid copying PathBufs everywhere
    #[serde(skip)]
    file: FileHandle,

    value: Spanned<T>,
}

struct Guard;

impl<T> Located<T> {
    pub fn new(value: T, file: FileHandle, span: Range<usize>) -> Self {
        Self {
            file,
            value: Spanned::new(span, value),
        }
    }

    pub fn span(&self) -> Range<usize> {
        self.value.span()
    }

    pub fn file(&self) -> FileHandle {
        self.file
    }
    pub fn path(&self) -> &Path {
        self.file.path()
    }

    pub fn source(&self) -> &str {
        self.file.source()
    }

    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }

    pub fn get_ref(&self) -> &T {
        self.value.get_ref()
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
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

/// Allows deserialization of [Located] values; sets their [file]s to [file]
// TODO: better error return types?
pub fn with_file<R, F: FnOnce(&str) -> R>(
    file: impl AsRef<Path>,
    f: F,
) -> PackageResult<(R, FileHandle)> {
    let buf = file.as_ref().to_path_buf();
    let file_id = FileHandle::new(buf)?;

    let guard = Guard::new(file_id);
    let result: R = f(file_id.source());

    Ok((result, file_id))
}

impl<'de, T> Deserialize<'de> for Located<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value: Spanned<T> = Spanned::<T>::deserialize(deserializer)?;
        let file = PARSING_FILE.with_borrow(|f| {
            *f.as_ref()
                .expect("Located<T> should only be deserialized in with_file")
        });

        Ok(Self { file, value })
    }
}

impl<T> AsRef<T> for Located<T> {
    fn as_ref(&self) -> &T {
        self.value.as_ref()
    }
}

impl<T> AsMut<T> for Located<T> {
    fn as_mut(&mut self) -> &mut T {
        self.value.as_mut()
    }
}

impl<T: PartialEq> PartialEq for Located<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value.eq(&other.value)
    }
}

impl<T: Eq> Eq for Located<T> {}

impl<T: Hash> Hash for Located<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<T: PartialOrd> PartialOrd for Located<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

impl<T: Ord> Ord for Located<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}
