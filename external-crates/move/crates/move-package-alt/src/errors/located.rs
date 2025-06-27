use std::ops::Range;

use super::FileHandle;

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
