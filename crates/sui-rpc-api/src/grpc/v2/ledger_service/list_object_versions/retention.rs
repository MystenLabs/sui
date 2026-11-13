// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::LazyLock;

use crate::Result;
use crate::RpcError;
use crate::ledger_history::query_options::below_oldest_retained_version;
use crate::ledger_history::query_options::below_serving_floor;
use move_core_types::ident_str;
use move_core_types::language_storage::StructTag;
use move_core_types::language_storage::TypeTag;
use sui_protocol_config::Chain;
use sui_protocol_config::ProtocolConfig;
use sui_protocol_config::ProtocolVersion;
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SequenceNumber;
use sui_types::digests::TransactionDigest;
use sui_types::object::Object as StoredObject;
use sui_types::object::Owner;
use sui_types::storage::LedgerTxSeqDigest;
use sui_types::storage::ObjectOrTombstone;

use super::IndexedTip;
use super::removal_kind;
use super::removals::removal;
use crate::ledger_history::object_versions::ObjectRemovalKind;
use crate::reader::StateReader;

/// Whether transaction `digest` created the live version `object` under an ID that no earlier
/// object can have had (see [`StateReader::created_new_object_id`]). `created` is what the
/// transaction's effects list as created, or `None` if they are pruned, and `read_object` reads
/// an object version.
///
/// `object::new` derives each ID from the digest of the transaction that creates it and the
/// number of IDs that transaction created before it ([`ObjectID::derive_id`]). Dynamic field and
/// derived object IDs are hashes of a parent and a key instead, so they never match. Checking
/// every count a transaction can reach takes milliseconds, and checking for a claim reads an
/// object, so the effects, when kept, settle the common cases first: an object they do not list
/// as created; a new ID, which almost always derives from a count below the number of objects
/// they list as created; and a derived object, whose claim adds a `Claimed(id)` dynamic field to
/// its parent in the same transaction.
pub(super) fn created_under_new_id(
    digest: &TransactionDigest,
    object: &StoredObject,
    created: Option<&[(ObjectRef, Owner)]>,
    read_object: impl Fn(&ObjectID, SequenceNumber) -> Option<StoredObject>,
) -> bool {
    let object_id = object.id();
    if object.type_().is_some_and(|type_| type_.is_dynamic_field()) {
        return false;
    }
    if let Some(created) = created {
        if !created.iter().any(|((id, _, _), _)| *id == object_id) {
            return false;
        }
        if (0..created.len() as u64)
            .any(|created_before| ObjectID::derive_id(*digest, created_before) == object_id)
        {
            return true;
        }
        let claimed = created.iter().any(|((id, version, _), owner)| {
            matches!(owner, Owner::ObjectOwner(_))
                && read_object(id, *version)
                    .is_some_and(|marker| is_claim_marker(&marker, object_id))
        });
        if claimed {
            return false;
        }
    }
    (0..*MAX_IDS_CREATED_PER_TRANSACTION)
        .any(|created_before| ObjectID::derive_id(*digest, created_before) == object_id)
}

/// Whether `object` is the `sui::derived_object::Claimed(object_id)` dynamic field that marks
/// `object_id` as claimed under its parent: a `0x2::dynamic_field::Field<Claimed, _>`, whose
/// contents are its UID followed by its name, the claimed ID.
pub(super) fn is_claim_marker(object: &StoredObject, object_id: ObjectID) -> bool {
    let Some(move_object) = object.data.try_as_move() else {
        return false;
    };
    let type_ = move_object.type_();
    let claimed = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: ident_str!("derived_object").to_owned(),
        name: ident_str!("Claimed").to_owned(),
        type_params: vec![],
    }));
    type_.is_dynamic_field()
        && type_
            .type_params()
            .first()
            .is_some_and(|name| **name == claimed)
        && move_object
            .contents()
            .get(ObjectID::LENGTH..2 * ObjectID::LENGTH)
            == Some(object_id.as_ref())
}

/// The most IDs one transaction can create with `object::new`: the limit for system
/// transactions, which is above the one for user transactions.
pub(super) static MAX_IDS_CREATED_PER_TRANSACTION: LazyLock<u64> = LazyLock::new(|| {
    ProtocolConfig::get_for_version(ProtocolVersion::MAX, Chain::Unknown)
        .max_num_new_move_object_ids_system_tx()
});

/// The listed object's retention floor, as of the last read of the lower bound.
#[derive(Clone, Copy, Debug)]
pub(super) struct Retention {
    /// See [`read_floor`].
    pub(super) floor_checkpoint: Option<u64>,
    /// Whether the listing reports each removal's transaction, which raises the floor.
    lists_transactions: bool,
    /// What the object keeps of its history above `floor_checkpoint`.
    kept: KeptHistory,
}

/// The listing's floor checkpoint, or `None` when nothing is pruned: the object floor
/// ([`crate::reader::AtCheckpointLowerBound::pruned_floor`]), raised, when the listing reports
/// removal transactions, to one below the lowest checkpoint with transaction data. The two differ
/// only on nodes that keep every object version and prune checkpoints, whose removals at or
/// below the higher floor may have no effects left to find their transactions by.
pub(super) fn read_floor(reader: &StateReader, lists_transactions: bool) -> Result<Option<u64>> {
    let object_floor = reader.at_checkpoint_lower_bound()?.pruned_floor();
    let lowest_with_transactions = if lists_transactions {
        Some(reader.get_lowest_available_checkpoint()?)
    } else {
        None
    };
    Ok(listing_floor(object_floor, lowest_with_transactions))
}

/// [`read_floor`], given the object floor and, for a listing that reports removal transactions,
/// the lowest checkpoint with transaction data: the checkpoint before that one, when higher.
fn listing_floor(object_floor: Option<u64>, lowest_with_transactions: Option<u64>) -> Option<u64> {
    object_floor.max(lowest_with_transactions.and_then(|lowest| lowest.checked_sub(1)))
}

/// The version of the tombstone the object store holds for the removal the version index records
/// at `indexed`, or `None` if the item there is a live version: the first stored item at or
/// above `indexed` (see [`super::stored_version_at`]).
fn removal_stored_at(
    reader: &StateReader,
    object_id: ObjectID,
    indexed: SequenceNumber,
) -> Result<Option<SequenceNumber>> {
    let first_stored = reader
        .object_versions_iter(object_id, indexed..=SequenceNumber::MAX, false)?
        .next()
        .transpose()?;
    Ok(match first_stored {
        Some(ObjectOrTombstone::Tombstone((_, version, _))) => Some(version),
        _ => None,
    })
}

/// The version of the object's first item, live or a removal, above `version` and at or below
/// the indexed tip.
fn first_item_after(
    reader: &StateReader,
    object_id: ObjectID,
    version: SequenceNumber,
    tip: &IndexedTip,
) -> Result<Option<SequenceNumber>> {
    let after = super::next_version(version);
    if after > tip.newest_version {
        return Ok(None);
    }
    Ok(reader
        .object_versions_iter(object_id, after..=tip.newest_version, false)?
        .next()
        .transpose()?
        .map(|item| super::stored_version(&item)))
}

/// What an object keeps of its history above the retention floor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum KeptHistory {
    /// Its whole history: nothing is pruned, or the object was created after the floor
    /// checkpoint under a new ID.
    Whole,
    /// The versions from this one on.
    From(OldestRetainedVersion),
    /// Nothing: pruning removed every item of the object up to the indexed tip, which happens
    /// when it raises the floor past the object's last removal while the object is listed.
    Nothing { floor_checkpoint: u64 },
}

/// The oldest version of an object this node keeps, when it pruned older ones: its version at the
/// end of the floor checkpoint, or for an object with none, which was deleted or wrapped at or
/// before it, its first item after it, live or a removal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct OldestRetainedVersion {
    pub(super) floor_checkpoint: u64,
    pub(super) version: SequenceNumber,
}

impl OldestRetainedVersion {
    pub(super) fn below_error(&self) -> RpcError {
        below_oldest_retained_version(self.floor_checkpoint, self.version.value())
    }
}

impl Retention {
    /// The floor, and what the object keeps above it. Pruning can raise the floor while the
    /// object's history is read, and a read below the new floor may find the object's rows
    /// retracted, so the floor is read again after it, until it has not moved.
    pub(super) fn read(
        reader: &StateReader,
        object_id: ObjectID,
        tip: &IndexedTip,
        lists_transactions: bool,
    ) -> Result<Self> {
        let mut floor_checkpoint = read_floor(reader, lists_transactions)?;
        loop {
            let kept = match floor_checkpoint {
                Some(floor_checkpoint) => {
                    // Pruning never passes the indexed tip, so the floor is within the upper
                    // bound.
                    let at_floor = reader.get_object_version_at_checkpoint(
                        object_id,
                        floor_checkpoint,
                        &reader.at_checkpoint_upper_bound()?,
                    )?;
                    match at_floor {
                        Some(version) => match lists_transactions
                            .then(|| removal_stored_at(reader, object_id, version))
                            .transpose()?
                            .flatten()
                        {
                            // Above the object floor the index still holds a removal at or before
                            // the floor checkpoint, whose transaction a listing that reports
                            // transactions cannot find. Every item above it was written after the
                            // floor checkpoint, so the first is the oldest retained one.
                            Some(removed) => {
                                match first_item_after(reader, object_id, removed, tip)? {
                                    Some(version) => KeptHistory::From(OldestRetainedVersion {
                                        floor_checkpoint,
                                        version,
                                    }),
                                    None => KeptHistory::Nothing { floor_checkpoint },
                                }
                            }
                            None => KeptHistory::From(OldestRetainedVersion {
                                floor_checkpoint,
                                version,
                            }),
                        },
                        None => first_retained_version_after_removal(
                            reader,
                            object_id,
                            floor_checkpoint,
                            tip,
                        )?,
                    }
                }
                None => KeptHistory::Whole,
            };
            let floor_after = read_floor(reader, lists_transactions)?;
            if floor_after == floor_checkpoint {
                return Ok(Self {
                    floor_checkpoint,
                    lists_transactions,
                    kept,
                });
            }
            floor_checkpoint = floor_after;
        }
    }

    /// The object's oldest retained version, when versions before it are pruned. `None` when
    /// nothing is pruned, or the object was created after the floor checkpoint and keeps its
    /// whole history. Fails with [`below_serving_floor`] when the object keeps nothing.
    pub(super) fn oldest_retained(&self) -> Result<Option<OldestRetainedVersion>> {
        match self.kept {
            KeptHistory::Whole => Ok(None),
            KeptHistory::From(oldest_retained) => Ok(Some(oldest_retained)),
            KeptHistory::Nothing { floor_checkpoint } => Err(below_serving_floor(floor_checkpoint)),
        }
    }

    /// After reading versions down to `lowest_read`: the floor again, failing if pruning has
    /// since raised the object's oldest retained version above `lowest_read`, or removed every
    /// item of the object, since the read may have missed versions pruned while it ran. Pruning
    /// raises the floor no later than it deletes versions, so a floor read after the versions
    /// that is still at or below `lowest_read` shows that the read saw every version above it.
    pub(super) fn recheck(
        self,
        reader: &StateReader,
        object_id: ObjectID,
        lowest_read: SequenceNumber,
        tip: &IndexedTip,
    ) -> Result<Self> {
        let floor_checkpoint = read_floor(reader, self.lists_transactions)?;
        let retention = if floor_checkpoint == self.floor_checkpoint {
            self
        } else {
            Self::read(reader, object_id, tip, self.lists_transactions)?
        };
        if let Some(oldest_retained) = retention.oldest_retained()?
            && lowest_read < oldest_retained.version
        {
            return Err(oldest_retained.below_error());
        }
        Ok(retention)
    }
}

/// For an object with no version at the end of `floor_checkpoint`: its first item written after
/// that checkpoint, live or a removal, when the history before it is pruned; its whole history
/// when the transaction that wrote that item created the object under a new ID; or nothing when
/// it has no such item, because pruning removed every item up to the indexed tip.
///
/// Such an object was created after the floor checkpoint, or removed at or before it and then
/// unwrapped, or created again under the same ID, after it. Pruning dropped every index row of
/// the removed object. A first live version created under a new ID keeps the whole history (see
/// [`StateReader::created_new_object_id`]). Anything else is taken as the floor: an unwrap, a
/// creation under an ID that may have been used before, and a first removal, since the effects
/// of a transaction that unwrapped and deleted the object do not show whether it was created
/// wrapped.
///
/// Items written at or before the floor checkpoint, which pruning has not deleted yet (or, for
/// tombstones, never deletes while `killswitch_tombstone_pruning` is set), are skipped:
/// - a live version whose transaction ran in a checkpoint at or before it;
/// - a wrap: one written after the floor checkpoint follows a live version written after it too,
///   since the object has no version at its end, and that version comes first;
/// - a deletion whose transaction ran in a checkpoint at or before it, or can no longer be found
///   (see [`removal`]).
///
/// Items are placed through the rpc indexes: the sequence number of the transaction that wrote
/// an item, and that transaction's checkpoint. The indexes prune both in the same write that
/// raises the floor, so an item they can no longer place was written at or before the floor
/// checkpoint, and is skipped too. A deletion whose effects are pruned is placed by its
/// unconfirmed [`removal`], which may be a later creation of the object straight into a wrapper.
/// If that creation ran after the floor checkpoint and the deletion at or before it, the listing
/// starts at the stored deletion, every item after which was written after the floor checkpoint.
pub(super) fn first_retained_version_after_removal(
    reader: &StateReader,
    object_id: ObjectID,
    floor_checkpoint: u64,
    tip: &IndexedTip,
) -> Result<KeptHistory> {
    let retained_from = |version| {
        KeptHistory::From(OldestRetainedVersion {
            floor_checkpoint,
            version,
        })
    };
    let after_floor = |transaction: Option<LedgerTxSeqDigest>| {
        transaction.is_some_and(|transaction| transaction.checkpoint_number > floor_checkpoint)
    };
    // The first item is almost always the answer, and the iterator decodes only the items taken.
    for item in
        reader.object_versions_iter(object_id, SequenceNumber::MIN..=tip.newest_version, false)?
    {
        match item? {
            ObjectOrTombstone::Object(object) => {
                let digest = object.previous_transaction;
                let transaction = match reader.transaction_sequence(&digest)? {
                    Some(sequence) => reader.ledger_transaction(sequence)?,
                    None => None,
                };
                if after_floor(transaction) {
                    return Ok(if reader.created_new_object_id(&digest, &object) {
                        KeptHistory::Whole
                    } else {
                        retained_from(object.version())
                    });
                }
            }
            ObjectOrTombstone::Tombstone((_, version, digest)) => {
                if removal_kind(&digest)? == ObjectRemovalKind::Deleted
                    && after_floor(
                        removal(reader, object_id, version, tip)?
                            .map(|removal| removal.transaction),
                    )
                {
                    return Ok(retained_from(version));
                }
            }
        }
    }
    Ok(KeptHistory::Nothing { floor_checkpoint })
}

#[cfg(test)]
mod tests {
    use sui_types::base_types::SuiAddress;

    use super::*;

    /// A listing that reports removal transactions starts from the state at the end of the
    /// checkpoint before the lowest one with transaction data, when that is above the object
    /// floor; otherwise, and for every other listing, at the end of the object floor.
    #[test]
    fn listing_floor_is_the_checkpoint_before_transaction_data_when_higher() {
        for (object_floor, lowest_with_transactions, expected) in [
            (Some(3), Some(10), Some(9)),
            (Some(3), None, Some(3)),
            (Some(12), Some(10), Some(12)),
            (None, Some(10), Some(9)),
            (None, Some(0), None),
            (None, None, None),
        ] {
            assert_eq!(
                listing_floor(object_floor, lowest_with_transactions),
                expected,
                "object floor {object_floor:?}, transactions from {lowest_with_transactions:?}"
            );
        }
    }

    /// A Move object of type `type_` with `contents`, owned by `owner`, written by `digest`.
    fn move_object(
        type_: StructTag,
        contents: Vec<u8>,
        owner: Owner,
        digest: TransactionDigest,
    ) -> StoredObject {
        // SAFETY: these types are not coins, and the tests read no transfer abilities.
        let move_object = unsafe {
            sui_types::object::MoveObject::new_from_execution_with_limit(
                type_.into(),
                false,
                SequenceNumber::from_u64(5),
                contents,
                u64::MAX,
            )
        }
        .unwrap();
        StoredObject::new_move(move_object, owner, digest)
    }

    /// An object created by `object::new` has an ID derived from its transaction's digest, which
    /// shows a new ID whether or not the effects are kept. A derived object, claimed or not, a
    /// dynamic field, and an object its transaction did not create have no new ID.
    #[test]
    fn created_under_new_id_tells_reusable_ids_apart() {
        let digest = TransactionDigest::random();
        let parent = ObjectID::random();
        let owned = Owner::AddressOwner(SuiAddress::ZERO);
        let created = |object: &StoredObject, owner: Owner| {
            ((object.id(), object.version(), object.digest()), owner)
        };

        let mut fresh = StoredObject::with_id_owner_version_for_testing(
            ObjectID::derive_id(digest, 2),
            SequenceNumber::from_u64(5),
            owned.clone(),
        );
        fresh.previous_transaction = digest;
        // A derived object's ID is a hash of its parent and key.
        let mut derived = StoredObject::with_id_owner_version_for_testing(
            ObjectID::random(),
            SequenceNumber::from_u64(5),
            owned.clone(),
        );
        derived.previous_transaction = digest;
        let claimed_tag = |name: &str| {
            TypeTag::Struct(Box::new(StructTag {
                address: SUI_FRAMEWORK_ADDRESS,
                module: ident_str!("derived_object").to_owned(),
                name: move_core_types::identifier::Identifier::new(name).unwrap(),
                type_params: vec![],
            }))
        };
        let marker_contents =
            [ObjectID::random().to_vec(), derived.id().to_vec(), vec![0]].concat();
        let marker = move_object(
            sui_types::dynamic_field::DynamicFieldInfo::dynamic_field_type(
                claimed_tag("Claimed"),
                claimed_tag("ClaimedStatus"),
            ),
            marker_contents,
            Owner::ObjectOwner(parent.into()),
            digest,
        );
        let field_id = ObjectID::derive_id(digest, 3);
        let field = move_object(
            sui_types::dynamic_field::DynamicFieldInfo::dynamic_field_type(
                TypeTag::U64,
                TypeTag::U64,
            ),
            [
                field_id.to_vec(),
                0u64.to_le_bytes().to_vec(),
                0u64.to_le_bytes().to_vec(),
            ]
            .concat(),
            Owner::ObjectOwner(parent.into()),
            digest,
        );

        let read_marker =
            |id: &ObjectID, _version: SequenceNumber| (*id == marker.id()).then(|| marker.clone());
        let with_marker = [
            created(&fresh, owned.clone()),
            created(&derived, owned),
            created(&marker, Owner::ObjectOwner(parent.into())),
        ];

        for (object, created, expected) in [
            (&fresh, Some(&with_marker[..]), true),
            (&fresh, None, true),
            (&fresh, Some(&with_marker[2..]), false),
            (&derived, Some(&with_marker[..]), false),
            (&derived, None, false),
            (&field, None, false),
        ] {
            assert_eq!(
                created_under_new_id(&digest, object, created, read_marker),
                expected,
                "{} with effects {:?}",
                object.id(),
                created.map(<[_]>::len)
            );
        }
        assert!(is_claim_marker(&marker, derived.id()));
        assert!(!is_claim_marker(&marker, fresh.id()));
    }
}
