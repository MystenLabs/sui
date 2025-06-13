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
    loc: Location,
    value: T,
}

#[derive(Debug, Clone)]
pub struct Location {
    file: FileHandle,
    span: Range<usize>,
}

impl Location {
    pub fn new(file: FileHandle, span: Range<usize>) -> Self {
        Self { file, span }
    }

    pub fn file(&self) -> FileHandle {
        self.file
    }

    pub fn span(&self) -> &Range<usize> {
        &self.span
    }
}

impl<T> Located<T> {
    pub fn new(value: T, file: FileHandle, span: Range<usize>) -> Self {
        Self {
            loc: Location::new(file, span),
            value,
        }
    }

    pub fn location(&self) -> &Location {
        &self.loc
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
        let span = value.span();
        Ok(Self {
            value: value.into_inner(),
            loc: Location::new(file, span),
        })
    }
}

impl<T> AsRef<T> for Located<T> {
    fn as_ref(&self) -> &T {
        &self.value
    }
}

impl<T> AsMut<T> for Located<T> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T: PartialEq> PartialEq for Located<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value.eq(&other.value)
    }
}
