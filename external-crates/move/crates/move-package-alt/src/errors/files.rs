use std::{
    fmt::{Debug, Display},
    fs, io,
    path::{Path, PathBuf},
};

use append_only_vec::AppendOnlyVec;
use codespan_reporting::files::SimpleFile;
use serde::de::DeserializeOwned;

/// A wrapper around [PathBuf] that implements [Display]
#[derive(Clone)]
pub struct DisplayPath(PathBuf);

/// A collection that holds a file path and its contents. This is used for when parsing the
/// manifest and lockfiles to enable nice reporting throughout the system.
static FILES: AppendOnlyVec<SimpleFile<DisplayPath, String>> = AppendOnlyVec::new();

/// An implementation of [codespan_reporting::files::Files] for [FileHandle]s (using the
/// global cache)
pub struct Files;

/// A cheap handle into a global collection of files
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct FileHandle {
    /// Invariant: guaranteed to be in FILES
    id: usize,
}

impl<'a> codespan_reporting::files::Files<'a> for Files {
    type FileId = FileHandle;
    type Name = &'a DisplayPath;
    type Source = &'a String;

    fn name(&'a self, id: Self::FileId) -> Result<Self::Name, codespan_reporting::files::Error> {
        Ok(id.simple_file().name())
    }

    fn source(
        &'a self,
        id: Self::FileId,
    ) -> Result<Self::Source, codespan_reporting::files::Error> {
        Ok(id.simple_file().source())
    }

    fn line_index(
        &'a self,
        id: Self::FileId,
        byte_index: usize,
    ) -> Result<usize, codespan_reporting::files::Error> {
        id.simple_file().line_index((), byte_index)
    }

    fn line_range(
        &'a self,
        id: Self::FileId,
        line_index: usize,
    ) -> Result<std::ops::Range<usize>, codespan_reporting::files::Error> {
        id.simple_file().line_range((), line_index)
    }
}

impl FileHandle {
    /// Reads the file located at [path] into the file cache and returns its ID
    pub fn new(path: impl AsRef<Path>) -> io::Result<Self> {
        let source = fs::read_to_string(&path)?;

        let id = FILES.push(SimpleFile::new(
            DisplayPath(path.as_ref().to_path_buf()),
            source,
        ));
        Ok(Self { id })
    }

    /// Return the path to the file at [id]
    pub fn path(&self) -> &'static Path {
        &FILES[self.id].name().0
    }

    /// Return the source code for the file at [id]
    pub fn source(&self) -> &'static String {
        FILES[self.id].source()
    }

    /// Parse `self` as a toml value of type T
    pub fn parse_toml<T: DeserializeOwned>(&self) -> Result<T, toml_edit::de::Error> {
        toml_edit::de::from_str(self.source())
    }

    /// Return a dummy file for test scaffolding
    #[cfg(test)]
    pub fn dummy(path: impl AsRef<Path>, contents: impl AsRef<str>) -> Self {
        let id = FILES.push(SimpleFile::new(
            DisplayPath(path.as_ref().to_path_buf()),
            contents.as_ref().to_string(),
        ));
        Self { id }
    }

    fn simple_file(&self) -> &'static SimpleFile<DisplayPath, String> {
        &FILES[self.id]
    }
}

impl Debug for FileHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.path().fmt(f)
    }
}

impl Display for FileHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl Display for DisplayPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<Path> for FileHandle {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}
