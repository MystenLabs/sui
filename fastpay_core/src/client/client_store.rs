use super::*;

use rocksdb::Options;
use serde::{Deserialize, Serialize};

use std::path::Path;
use typed_store::rocks::{open_cf, DBMap};
use typed_store::traits::Map;

const PENDING_TRANSFER_KEY: &str = "PENDING_TRANSFER_KEY";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthorityConfigStore {
    #[serde(
        serialize_with = "address_as_hex",
        deserialize_with = "address_from_hex"
    )]
    pub address: FastPayAddress,
    pub host: String,
    pub base_port: u32,
    pub database_path: String,
    pub voting_rights: usize,
}

pub struct ClientStore {
    /// Confirmed objects with it's ref owned by the client.
    object_id_to_object_ref: DBMap<ObjectID, ObjectRef>,
    /// TX Digest <-> object id linking map.
    object_id_to_tx_digests: DBMap<ObjectID, Vec<TransactionDigest>>,
    /// The known objects with it's sequence number owned by the client.
    object_id_to_sequence_number: DBMap<ObjectID, SequenceNumber>,
    /// Known certificates, indexed by TX digest.
    tx_digest_to_cert_order: DBMap<TransactionDigest, CertifiedOrder>,
    /// Pending transfer flag
    pending_transfer: DBMap<String, Order>,
}

impl ClientStore {
    /// Open an client store by directory path
    pub fn open<P: AsRef<Path>>(path: P, db_options: Option<Options>) -> ClientStore {
        let db = open_cf(
            &path,
            db_options,
            &[
                "object_id_to_object_ref",
                "object_id_to_tx_digests",
                "object_id_to_sequence_number",
                "tx_digest_to_cert_order",
                "authority_name_to_authority_clients",
                "pending_transfer",
            ],
        )
        .expect("Cannot open DB.");
        ClientStore {
            object_id_to_object_ref: DBMap::reopen(&db, Some("object_id_to_object_ref"))
                .expect("Cannot open object_id_to_object_ref CF."),
            object_id_to_tx_digests: DBMap::reopen(&db, Some("object_id_to_tx_digests"))
                .expect("Cannot open object_id_to_tx_digests CF."),
            object_id_to_sequence_number: DBMap::reopen(&db, Some("object_id_to_sequence_number"))
                .expect("Cannot open object_id_to_sequence_number CF."),
            tx_digest_to_cert_order: DBMap::reopen(&db, Some("tx_digest_to_cert_order"))
                .expect("Cannot open tx_digest_to_cert_order CF."),
            pending_transfer: DBMap::reopen(&db, Some("pending_transfer"))
                .expect("Cannot open pending_transfer CF."),
        }
    }

    ///
    /// For dealing with transaction digests
    ///

    /// Get the transcation digest for a given object
    pub fn get_tx_digests(
        &self,
        object_id: &ObjectID,
    ) -> Result<Vec<TransactionDigest>, FastPayError> {
        Ok(match self.object_id_to_tx_digests.get(object_id)? {
            Some(r) => r,
            None => Vec::new(),
        })
    }
    pub fn remove_tx_digests(&self, object_id: &ObjectID) -> Result<(), FastPayError> {
        // If removal fails, still fail?
        Ok(self.object_id_to_tx_digests.remove(object_id)?)
    }
    pub fn insert_tx_digest(
        &self,
        object_id: &ObjectID,
        digest: &TransactionDigest,
    ) -> Result<(), FastPayError> {
        let mut d = self.get_tx_digests(object_id)?;
        // We should probably use a set if possible, not a vec
        if !d.contains(digest) {
            d.push(*digest);
        }
        Ok(self.object_id_to_tx_digests.insert(object_id, &d)?)
    }

    ///
    /// For dealing with object refs
    ///

    /// Get the object refs for a given object
    pub fn get_object_ref(&self, object_id: ObjectID) -> Result<Option<ObjectRef>, FastPayError> {
        Ok(self.object_id_to_object_ref.get(&object_id)?)
    }
    /// Get all object refs
    pub fn get_all_object_refs(&self) -> Result<BTreeMap<ObjectID, ObjectRef>, FastPayError> {
        let v: BTreeMap<ObjectID, ObjectRef> = self.object_id_to_object_ref.iter().collect();
        Ok(v)
    }
    pub fn remove_object_ref(&self, object_id: &ObjectID) -> Result<(), FastPayError> {
        // If removal fails, still fail?
        Ok(self.object_id_to_object_ref.remove(object_id)?)
    }
    pub fn clear_object_refs(&self) -> Result<(), FastPayError> {
        // Need to delete by range. No easy way to do this
        // TODO: need to implement https://github.com/MystenLabs/mysten-infra/issues/7
        let keys = self.object_id_to_object_ref.keys();
        let mut batch = self.object_id_to_object_ref.batch();
        batch = batch.delete_batch(&self.object_id_to_object_ref, keys)?;
        batch.write().map_err(|e| e.into())
    }
    pub fn insert_object_ref(
        &self,
        object_id: &ObjectID,
        object_ref: &ObjectRef,
    ) -> Result<(), FastPayError> {
        Ok(self.object_id_to_object_ref.insert(object_id, object_ref)?)
    }

    ///
    /// For dealing with sequence numbers
    ///

    /// Get the sequence numbers for a given object
    pub fn get_sequence_number(
        &self,
        object_id: ObjectID,
    ) -> Result<Option<SequenceNumber>, FastPayError> {
        Ok(self.object_id_to_sequence_number.get(&object_id)?)
    }
    /// Get all sequence numbers
    pub fn get_all_sequence_numbers(
        &self,
    ) -> Result<BTreeMap<ObjectID, SequenceNumber>, FastPayError> {
        let v: BTreeMap<ObjectID, SequenceNumber> =
            self.object_id_to_sequence_number.iter().collect();
        Ok(v)
    }
    pub fn remove_sequence_number(&self, object_id: &ObjectID) -> Result<(), FastPayError> {
        // If removal fails, still fail?
        Ok(self.object_id_to_sequence_number.remove(object_id)?)
    }
    pub fn insert_sequence_number(
        &self,
        object_id: &ObjectID,
        seq_no: &SequenceNumber,
    ) -> Result<(), FastPayError> {
        Ok(self
            .object_id_to_sequence_number
            .insert(object_id, seq_no)?)
    }
    pub fn clear_sequence_numbers(&self) -> Result<(), FastPayError> {
        // Need to delete by range. No easy way to do this
        // TODO: need to implement https://github.com/MystenLabs/mysten-infra/issues/7
        let keys = self.object_id_to_sequence_number.keys();
        let mut batch = self.object_id_to_sequence_number.batch();
        batch = batch.delete_batch(&self.object_id_to_sequence_number, keys)?;
        batch.write().map_err(|e| e.into())
    }

    ///
    /// For dealing with transaction digests and certs
    ///

    /// Get the sequence numbers for a given object
    pub fn get_certified_order(
        &self,
        digest: TransactionDigest,
    ) -> Result<Option<CertifiedOrder>, FastPayError> {
        Ok(self.tx_digest_to_cert_order.get(&digest)?)
    }
    /// Get all sequence numbers
    pub fn get_all_certified_orders(
        &self,
    ) -> Result<BTreeMap<TransactionDigest, CertifiedOrder>, FastPayError> {
        let v: BTreeMap<TransactionDigest, CertifiedOrder> =
            self.tx_digest_to_cert_order.iter().collect();
        Ok(v)
    }
    pub fn _remove_certified_order(&self, digest: &TransactionDigest) -> Result<(), FastPayError> {
        // If removal fails, still fail?
        Ok(self.tx_digest_to_cert_order.remove(digest)?)
    }
    pub fn insert_certified_order(
        &self,
        digest: &TransactionDigest,
        seq_no: &CertifiedOrder,
    ) -> Result<(), FastPayError> {
        Ok(self.tx_digest_to_cert_order.insert(digest, seq_no)?)
    }

    ///
    /// For dealing with pending transfers
    ///

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
