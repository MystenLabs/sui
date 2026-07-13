// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use typed_store_error::TypedStoreError;

pub type Result<T, E = Error> = ::std::result::Result<T, E>;

#[derive(Debug)]
pub struct Error {
    inner: Box<Inner>,
}

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Debug)]
struct Inner {
    kind: Kind,
    source: Option<BoxError>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Serialization,
    Missing,
    Custom,

    /// The data exists (or may exist) but the store is currently unwilling to serve it, e.g.
    /// because the pipeline that produces it is disabled or lagging. Distinct from `Missing` so
    /// consumers can render it as a retriable "unavailable" condition rather than "not found".
    Unavailable,
}

impl Error {
    fn new<E: Into<BoxError>>(kind: Kind, source: Option<E>) -> Self {
        Self {
            inner: Box::new(Inner {
                kind,
                source: source.map(Into::into),
            }),
        }
    }

    pub fn serialization<E: Into<BoxError>>(e: E) -> Self {
        Self::new(Kind::Serialization, Some(e))
    }

    pub fn missing<E: Into<BoxError>>(e: E) -> Self {
        Self::new(Kind::Missing, Some(e))
    }

    pub fn custom<E: Into<BoxError>>(e: E) -> Self {
        Self::new(Kind::Custom, Some(e))
    }

    pub fn unavailable<E: Into<BoxError>>(e: E) -> Self {
        Self::new(Kind::Unavailable, Some(e))
    }

    pub fn kind(&self) -> Kind {
        self.inner.kind
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner.source.as_ref().map(|e| &**e as _)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO: change output based on kind?
        write!(f, "{:?}", self)
    }
}

impl From<TypedStoreError> for Error {
    fn from(e: TypedStoreError) -> Self {
        Self::custom(e)
    }
}
