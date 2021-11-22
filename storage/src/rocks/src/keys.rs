use std::marker::PhantomData;

use bincode::Options;
use serde::de::DeserializeOwned;

/// An iterator over the keys of a prefix.
pub struct Keys<'a, K> {
    db_iter: rocksdb::DBRawIterator<'a>,
    _phantom: PhantomData<K>,
}

impl<'a, K: DeserializeOwned> Keys<'a, K> {
    pub(crate) fn new(db_iter: rocksdb::DBRawIterator<'a>) -> Self {
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
            let config = bincode::DefaultOptions::new().with_big_endian();
            let key = self.db_iter.key().and_then(|k| config.deserialize(k).ok());

            self.db_iter.next();
            key
        } else {
            None
        }
    }
}