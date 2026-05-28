// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::error::{BoxError, ExecutionError, ExecutionErrorMetadata, ExecutionErrorTrait};
use sui_types::execution_status::{CommandIndex, ExecutionErrorKind, ExecutionFailure};

#[derive(Debug)]
pub struct ExecutionErrorContext {
    kind: ExecutionErrorKind,
    metadata: ExecutionErrorMetadata,
    source: Option<BoxError>,
    command: Option<CommandIndex>,
}

impl ExecutionErrorContext {
    pub fn kind(&self) -> &ExecutionErrorKind {
        &self.kind
    }

    pub fn command(&self) -> Option<CommandIndex> {
        self.command
    }

    pub fn metadata_with_source(&self) -> Option<ExecutionErrorMetadata> {
        let mut metadata = self.metadata.clone();
        if let Some(source) = self.source.as_ref() {
            metadata.message.get_or_insert_with(|| source.to_string());
        }

        (!metadata.is_empty()).then_some(metadata)
    }

    pub fn to_execution_status(&self) -> (ExecutionErrorKind, Option<CommandIndex>) {
        (self.kind().clone(), self.command())
    }
}

impl ExecutionErrorTrait for ExecutionErrorContext {
    fn new(
        failure: ExecutionFailure,
        source: Option<BoxError>,
        metadata: ExecutionErrorMetadata,
    ) -> Self {
        let ExecutionFailure { error, command } = failure;
        Self {
            kind: error,
            metadata,
            source,
            command,
        }
    }

    fn with_command_index(self, command: CommandIndex) -> Self {
        Self {
            command: Some(command),
            ..self
        }
    }

    fn kind(&self) -> &ExecutionErrorKind {
        self.kind()
    }

    fn command(&self) -> Option<CommandIndex> {
        self.command()
    }
}

impl std::fmt::Display for ExecutionErrorContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ExecutionErrorContext: {:?}", self)
    }
}

impl std::error::Error for ExecutionErrorContext {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_deref().map(|e| e as _)
    }
}

impl From<ExecutionErrorKind> for ExecutionErrorContext {
    fn from(kind: ExecutionErrorKind) -> Self {
        <Self as ExecutionErrorTrait>::from_kind(kind)
    }
}

impl From<ExecutionFailure> for ExecutionErrorContext {
    fn from(value: ExecutionFailure) -> Self {
        <Self as ExecutionErrorTrait>::from_execution_failure(value)
    }
}

impl From<ExecutionError> for ExecutionErrorContext {
    fn from(value: ExecutionError) -> Self {
        let (kind, source, command) = value.into_parts();
        Self {
            kind,
            metadata: ExecutionErrorMetadata::default(),
            source,
            command,
        }
    }
}

impl From<ExecutionErrorContext> for ExecutionError {
    fn from(value: ExecutionErrorContext) -> Self {
        let ExecutionErrorContext {
            kind,
            metadata: _,
            source,
            command,
        } = value;
        let err = ExecutionError::new(kind, source);
        if let Some(command) = command {
            err.with_command_index(command)
        } else {
            err
        }
    }
}
