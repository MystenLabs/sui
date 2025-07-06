// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use std::marker::PhantomData;

use serde::{de::DeserializeOwned, Serialize};

use crate::{error::Error, key};

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
    /// `probe`. The probe type `J` can differ from the key type `K`, to allow for seeking
    /// prefixes.
    pub(crate) fn seek<J: Serialize>(&mut self, probe: &J) {
        if let Some(inner) = &mut self.inner {
            inner.seek(key::encode(probe));
        }
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
    /// `probe`. The probe type `J` can differ from the key type `K`, to allow for seeking
    /// prefixes.
    pub(crate) fn seek<J: Serialize>(&mut self, probe: &J) {
        if let Some(inner) = &mut self.inner {
            inner.seek_for_prev(key::encode(probe));
        }
    }
}

impl<K, V> Iterator for FwdIter<'_, K, V>
where
    K: DeserializeOwned,
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
    K: DeserializeOwned,
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

#[inline]
fn decode_item<K, V>(iter: &rocksdb::DBRawIterator<'_>) -> Option<Result<(K, V), Error>>
where
    K: DeserializeOwned,
    V: DeserializeOwned,
{
    if !iter.valid() {
        return if let Err(e) = iter.status() {
            Some(Err(Error::Storage(e)))
        } else {
            None
        };
    }

    let k = match key::decode(iter.key()?).map_err(Error::Bincode) {
        Ok(k) => k,
        Err(e) => return Some(Err(e)),
    };

    let v = match bcs::from_bytes(iter.value()?) {
        Ok(v) => v,
        Err(e) => return Some(Err(Error::Bcs(e))),
    };

    Some(Ok((k, v)))
}
