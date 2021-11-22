use std::marker::PhantomData;

use bincode::Options;
use serde::de::DeserializeOwned;

/// An iterator over all key-value pairs in a data map.
pub struct Iter<'a, K, V> {
    db_iter: rocksdb::DBRawIterator<'a>,
    _phantom: PhantomData<(K, V)>,
}

impl<'a, K: DeserializeOwned, V: DeserializeOwned> Iter<'a, K, V> {
    pub(super) fn new(db_iter: rocksdb::DBRawIterator<'a>) -> Self {
        Self {
            db_iter,
            _phantom: PhantomData,
        }
    }
}

impl<'a, K: DeserializeOwned, V: DeserializeOwned> Iterator for Iter<'a, K, V> {
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        if self.db_iter.valid() {
            let config = bincode::DefaultOptions::new().with_big_endian();
            let key = self.db_iter.key().and_then(|k| config.deserialize(k).ok());
            let value = self
                .db_iter
                .value()
                .and_then(|v| bincode::deserialize(v).ok());

            self.db_iter.next();
            key.and_then(|k| value.map(|v| (k, v)))
        } else {
            None
        }
    }
}