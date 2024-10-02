// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::{future::BoxFuture, FutureExt};
use opentelemetry::trace::TraceError;
use opentelemetry_proto::{
    tonic::collector::trace::v1::ExportTraceServiceRequest,
    transform::{
        common::tonic::ResourceAttributesWithSchema,
        trace::tonic::group_spans_by_resource_and_scope,
    },
};
use opentelemetry_sdk::export::trace::{ExportResult, SpanData, SpanExporter};
use prost::Message;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(crate) struct CachedOpenFile {
    inner: Arc<Mutex<Option<(PathBuf, std::fs::File)>>>,
}

impl std::fmt::Debug for CachedOpenFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedOpenFile").finish()
    }
}

impl CachedOpenFile {
    pub fn open_file(path: &Path) -> std::io::Result<std::fs::File> {
        OpenOptions::new().append(true).create(true).open(path)
    }

    pub fn new<P: AsRef<Path>>(file_path: Option<P>) -> std::io::Result<Self> {
        let inner = if let Some(file_path) = file_path {
            let file_path = file_path.as_ref();
            let file = Self::open_file(file_path)?;
            Some((file_path.to_owned(), file))
        } else {
            None
        };
        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
        })
    }

    pub fn update_path<P: AsRef<Path>>(&self, file_path: P) -> std::io::Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let file_path = file_path.as_ref().to_owned();

        if let Some((old_file_path, _)) = &*inner {
            if old_file_path == &file_path {
                return Ok(());
            }
        }

        let file = Self::open_file(file_path.as_path())?;
        *inner = Some((file_path, file));
        Ok(())
    }

    pub fn clear_path(&self) {
        self.inner.lock().unwrap().take();
    }

    fn with_file(
        &self,
        f: impl FnOnce(Option<&mut std::fs::File>) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        f(self.inner.lock().unwrap().as_mut().map(|(_, file)| file))
    }
}

#[derive(Debug)]
pub(crate) struct FileExporter {
    pub cached_open_file: CachedOpenFile,
    resource: ResourceAttributesWithSchema,
}

impl FileExporter {
    pub fn new(file_path: Option<PathBuf>) -> std::io::Result<Self> {
        Ok(Self {
            cached_open_file: CachedOpenFile::new(file_path)?,
            resource: ResourceAttributesWithSchema::default(),
        })
    }
}

impl SpanExporter for FileExporter {
    fn export(&mut self, batch: Vec<SpanData>) -> BoxFuture<'static, ExportResult> {
        let cached_open_file = self.cached_open_file.clone();
        let resource_spans = group_spans_by_resource_and_scope(batch, &self.resource);
        async move {
            cached_open_file
                .with_file(|maybe_file| {
                    if let Some(file) = maybe_file {
                        let request = ExportTraceServiceRequest { resource_spans };

                        let buf = request.encode_length_delimited_to_vec();

                        file.write_all(&buf)
                    } else {
                        Ok(())
                    }
                })
                .map_err(|e| TraceError::Other(e.into()))
        }
        .boxed()
    }
}
