// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use bincode::Options;

use serde::{de::DeserializeOwned, Serialize};
use std::marker::PhantomData;

use super::{be_fix_int_ser, DBRawIteratorMultiThreaded, TypedStoreError};

/// An iterator over the keys of a prefix.
pub struct Keys<'a, K> {
    db_iter: DBRawIteratorMultiThreaded<'a>,
    _phantom: PhantomData<K>,
}

impl<'a, K: DeserializeOwned> Keys<'a, K> {
    pub(crate) fn new(db_iter: DBRawIteratorMultiThreaded<'a>) -> Self {
        Self {
            db_iter,
            _phantom: PhantomData,
        }
    }
}

impl<'a, K: DeserializeOwned> Iterator for Keys<'a, K> {
    type Item = K;

    fn next(&mut self) -> Option<Self::Item> {
        if self.db_iter.valid() {
            let config = bincode::DefaultOptions::new()
                .with_big_endian()
                .with_fixint_encoding();
            let key = self.db_iter.key().and_then(|k| config.deserialize(k).ok());

            self.db_iter.next();
            key
        } else {
            None
        }
    }
}

impl<'a, K: Serialize> Keys<'a, K> {
    /// Skips all the elements that are smaller than the given key,
    /// and either lands on the key or the first one greater than
    /// the key.
    pub fn skip_to(mut self, key: &K) -> Result<Self, TypedStoreError> {
        self.db_iter.seek(be_fix_int_ser(key)?);
        Ok(self)
    }

    /// Moves the iterator the element given or
    /// the one prior to it if it does not exist. If there is
    /// no element prior to it, it returns an empty iterator.
    pub fn skip_prior_to(mut self, key: &K) -> Result<Self, TypedStoreError> {
        self.db_iter.seek_for_prev(be_fix_int_ser(key)?);
        Ok(self)
    }

    /// Seeks to the last key in the database (at this column family).
    ///
    pub fn skip_to_last(mut self) -> Self {
        self.db_iter.seek_to_last();
        self
    }
}
