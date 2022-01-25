use super::*;

use rocksdb::Options;

use std::path::Path;
use typed_store::rocks::{open_cf, DBMap};
use typed_store::traits::Map;

const PENDING_TRANSFER_KEY: &str = "PENDING_TRANSFER_KEY";

pub struct ClientStore {
    pending_transfer: DBMap<String, Order>,
}

impl ClientStore {
    /// Open an client store by directory path
    pub fn open<P: AsRef<Path>>(path: P, db_options: Option<Options>) -> ClientStore {
        let db = open_cf(&path, db_options, &["pending_transfer"]).expect("Cannot open DB.");
        ClientStore {
            pending_transfer: DBMap::reopen(&db, Some("pending_transfer"))
                .expect("Cannot open pending_transfer CF."),
        }
    }
    // Get the pending tx if any
    pub fn get_pending_transfer(&self) -> Result<Option<Order>, FastPayError> {
        self.pending_transfer
            .get(&PENDING_TRANSFER_KEY.to_string())
            .map_err(|e| e.into())
    }
    // Set the pending tx if any
    pub fn set_pending_transfer(&self, order: &Order) -> Result<(), FastPayError> {
        self.pending_transfer
            .insert(&PENDING_TRANSFER_KEY.to_string(), order)
            .map_err(|e| e.into())
    }
    pub fn clear_pending_transfer(&self) -> Result<(), FastPayError> {
        self.pending_transfer
            .remove(&PENDING_TRANSFER_KEY.to_string())
            .map_err(|e| e.into())
    }
}
