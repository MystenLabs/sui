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
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_spanned::Spanned;

use super::{FileHandle, PackageResult, TheFile};

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

    pub fn destructure(self) -> (T, FileHandle, Range<usize>) {
        let span = self.value.span();
        let value = self.value.into_inner();
        (value, self.file, span)
    }

    pub fn get_ref(&self) -> &T {
        self.value.get_ref()
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }
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
        let file = TheFile::handle();
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
