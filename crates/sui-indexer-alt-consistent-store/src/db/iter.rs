// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use std::marker::PhantomData;

use bincode::Decode;
use serde::de::DeserializeOwned;

use super::{error::Error, key};

/// An iterator that scans through elements in increasing key order.
pub(crate) struct FwdIter<'d, K, V> {
    inner: Option<rocksdb::DBRawIterator<'d>>,
    _data: PhantomData<(K, V)>,
}

/// An iterator that scans through elements in decreasing key order.
pub(crate) struct RevIter<'d, K, V> {
    inner: Option<rocksdb::DBRawIterator<'d>>,
    _data: PhantomData<(K, V)>,
}

impl<'d, K, V> FwdIter<'d, K, V> {
    pub(crate) fn new(inner: Option<rocksdb::DBRawIterator<'d>>) -> Self {
        Self {
            inner,
            _data: PhantomData,
        }
    }

    /// Move the iterator's cursor so that it will yield the first key greater than or equal to
    /// `probe`. The probe is a byte slice which does not have to be generated from the key type,
    /// `K` (e.g. it could be generated from a prefix).
    pub(crate) fn seek(&mut self, probe: impl AsRef<[u8]>) {
        if let Some(inner) = &mut self.inner {
            inner.seek(probe);
        }
    }

    /// Returns the raw bytes for the next key the iterator will yield, if any.
    pub(crate) fn raw_key(&self) -> Option<&[u8]> {
        self.inner.as_ref().and_then(|iter| iter.key())
    }

    /// Returns the raw bytes for the next value the iterator will yield, if any.
    pub(crate) fn raw_value(&self) -> Option<&[u8]> {
        self.inner.as_ref().and_then(|iter| iter.value())
    }

    /// Returns whether the underlying raw iterator is valid (i.e. will produce a next value) or
    /// not.
    pub(crate) fn valid(&self) -> bool {
        self.inner.as_ref().is_some_and(|iter| iter.valid())
    }
}

impl<'d, K, V> RevIter<'d, K, V> {
    pub(crate) fn new(inner: Option<rocksdb::DBRawIterator<'d>>) -> Self {
        Self {
            inner,
            _data: PhantomData,
        }
    }

    /// Move the iterator's cursor so that it will yield the first key less than or equal to
    /// `probe`. The probe is a byte slice which does not have to be generated from the key type,
    /// `K` (e.g. it could be generated from a prefix).
    pub(crate) fn seek(&mut self, probe: impl AsRef<[u8]>) {
        if let Some(inner) = &mut self.inner {
            inner.seek_for_prev(probe);
        }
    }

    /// Returns the raw bytes for the next key the iterator will yield, if any.
    pub(crate) fn raw_key(&self) -> Option<&[u8]> {
        self.inner.as_ref().and_then(|iter| iter.key())
    }

    /// Returns the raw bytes for the next value the iterator will yield, if any.
    pub(crate) fn raw_value(&self) -> Option<&[u8]> {
        self.inner.as_ref().and_then(|iter| iter.value())
    }

    /// Returns whether the underlying raw iterator is valid (i.e. will produce a next value) or
    /// not.
    pub(crate) fn valid(&self) -> bool {
        self.inner.as_ref().is_some_and(|iter| iter.valid())
    }
}

impl<K, V> Iterator for FwdIter<'_, K, V>
where
    K: Decode<()>,
    V: DeserializeOwned,
{
    type Item = Result<(K, V), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let Some(inner) = &mut self.inner else {
            return None;
        };

        match decode_item(inner)? {
            Err(e) => Some(Err(e)),
            Ok((k, v)) => {
                inner.next();
                Some(Ok((k, v)))
            }
        }
    }
}

impl<K, V> Iterator for RevIter<'_, K, V>
where
    K: Decode<()>,
    V: DeserializeOwned,
{
    type Item = Result<(K, V), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let Some(inner) = &mut self.inner else {
            return None;
        };

        match decode_item(inner)? {
            Err(e) => Some(Err(e)),
            Ok((k, v)) => {
                inner.prev();
                Some(Ok((k, v)))
            }
        }
    }
}

fn decode_item<K, V>(iter: &rocksdb::DBRawIterator<'_>) -> Option<Result<(K, V), Error>>
where
    K: Decode<()>,
    V: DeserializeOwned,
{
    if !iter.valid() {
        return if let Err(e) = iter.status() {
            Some(Err(Error::Storage(e)))
        } else {
            None
        };
    }

    let k = match key::decode(iter.key()?).map_err(Error::KeyDecode) {
        Ok(k) => k,
        Err(e) => return Some(Err(e)),
    };

    let v = match bcs::from_bytes(iter.value()?) {
        Ok(v) => v,
        Err(e) => return Some(Err(Error::Bcs(e))),
    };

    Some(Ok((k, v)))
}
