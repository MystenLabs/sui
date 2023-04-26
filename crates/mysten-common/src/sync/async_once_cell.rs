// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::{OwnedRwLockWriteGuard, RwLock};

/// This structure contains a cell for a single value.
/// The cell can be written only once, and can be read many times.
/// Readers are provided with async API, that waits for write to happen.
/// This is similar to tokio::sync::watch, except one difference:
/// * tokio::sync::watch requires existing receiver to work. If no subscriber is registered, and the value is sent to channel, the value is dropped
/// * Unlike with tokio::sync::watch, it is possible to write to AsyncOnceCell when no readers are registered, and value will be available later when AsyncOnceCell::get is called
pub struct AsyncOnceCell<T> {
    value: Arc<RwLock<Option<T>>>,
    writer: Mutex<Option<OwnedRwLockWriteGuard<Option<T>>>>,
}

impl<T: Send + Clone> AsyncOnceCell<T> {
    pub fn new() -> Self {
        let value = Arc::new(RwLock::new(None));
        let writer = value
            .clone()
            .try_write_owned()
            .expect("Write lock can not fail here");
        let writer = Mutex::new(Some(writer));
        Self { value, writer }
    }

    pub async fn get(&self) -> T {
        self.value
            .read()
            .await
            .as_ref()
            .cloned()
            .expect("Value is available when writer is dropped")
    }

    /// Sets the value and notifies waiters. Return error if called twice
    #[allow(clippy::result_unit_err)]
    pub fn set(&self, value: T) -> Result<(), ()> {
        let mut writer = self.writer.lock();
        match writer.take() {
            None => Err(()),
            Some(mut writer) => {
                *writer = Some(value);
                Ok(())
            }
        }
    }
}

impl<T: Send + Clone> Default for AsyncOnceCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn async_once_cell_test() {
        let cell = Arc::new(AsyncOnceCell::<u64>::new());
        let cell2 = cell.clone();
        let wait = tokio::spawn(async move { cell2.get().await });
        tokio::task::yield_now().await;
        cell.set(15).unwrap();
        assert!(cell.set(16).is_err());
        assert_eq!(15, cell.get().await);
        assert_eq!(15, wait.await.unwrap());
    }
}
