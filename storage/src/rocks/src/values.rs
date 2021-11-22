use std::marker::PhantomData;

use serde::de::DeserializeOwned;

/// An iterator over the values of a prefix.
pub struct Values<'a, V> {
    db_iter: rocksdb::DBRawIterator<'a>,
    _phantom: PhantomData<V>,
}

impl<'a, V: DeserializeOwned> Values<'a, V> {
    pub(crate) fn new(db_iter: rocksdb::DBRawIterator<'a>) -> Self {
        Self {
            db_iter,
            _phantom: PhantomData,
        }
    }
}

impl<'a, V: DeserializeOwned> Iterator for Values<'a, V> {
    type Item = V;

    fn next(&mut self) -> Option<Self::Item> {
        if self.db_iter.valid() {
            let value = self.db_iter.key().and_then(|_| {
                self.db_iter
                    .value()
                    .and_then(|v| bincode::deserialize(v).ok())
            });

            self.db_iter.next();
            value
        } else {
            None
        }
    }
}