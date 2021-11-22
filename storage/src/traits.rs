use eyre::Result;
use serde::{de::DeserializeOwned, Serialize};

pub trait Map<'a, K, V>
where
    K: Serialize + DeserializeOwned + ?Sized,
    V: Serialize + DeserializeOwned,
{
    type Iterator: Iterator<Item = (K, V)>;
    type Keys: Iterator<Item = K>;
    type Values: Iterator<Item = V>;

    /// Returns true if the map contains a value for the specified key.
    fn contains_key(&self, key: &K) -> Result<bool>;

    /// Returns the value for the given key from the map, if it exists.
    fn get(&self, key: &K) -> Result<Option<V>>;

    /// Returns the value for the given key from the map, if it exists
    /// or the given default value if it does not.
    fn get_or_insert<F: FnOnce() -> V>(&self, key: &K, default: F) -> Result<V> {
        self.get(key).and_then(|optv| match optv {
            Some(v) => Ok(v),
            None => {
                self.insert(key, &default())?;
                self.get(key).transpose().expect("default just inserted")
            }
        })
    }

    /// Inserts the given key-value pair into the map.
    fn insert(&self, key: &K, value: &V) -> Result<()>;

    /// Removes the entry for the given key from the map.
    fn remove(&self, key: &K) -> Result<()>;

    /// Returns an iterator visiting each key-value pair in the map.
    fn iter(&'a self) -> Self::Iterator;

    /// Returns an iterator over each key in the map.
    fn keys(&'a self) -> Self::Keys;

    /// Returns an iterator over each value in the map.
    fn values(&'a self) -> Self::Values;
}