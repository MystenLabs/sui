// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::package_store::PackageCache;
use crate::tables::{InputObjectKind, ObjectStatus, OwnerType};
use crate::FileType;
use crate::TRANSACTION_CONCURRENCY_LIMIT;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::stream::{self, StreamExt};
use move_core_types::annotated_value::{MoveStruct, MoveTypeLayout, MoveValue};
use move_core_types::language_storage::{StructTag, TypeTag};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use sui_package_resolver::{PackageStore, Resolver};
use sui_types::base_types::ObjectID;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::object::bounded_visitor::BoundedVisitor;
use sui_types::object::{Object, Owner};
use sui_types::transaction::TransactionData;
use sui_types::transaction::TransactionDataAPI;

pub mod checkpoint_handler;
pub mod df_handler;
pub mod event_handler;
pub mod move_call_handler;
pub mod object_handler;
pub mod package_handler;
pub mod transaction_bcs_handler;
pub mod transaction_handler;
pub mod transaction_objects_handler;
pub mod wrapped_object_handler;
const WRAPPED_INDEXING_DISALLOW_LIST: [&str; 4] = [
    "0x1::string::String",
    "0x1::ascii::String",
    "0x2::url::Url",
    "0x2::object::ID",
];

#[async_trait::async_trait]
pub trait AnalyticsHandler<S>: Send + Sync {
    /// Process a checkpoint and return a boxed iterator over the rows.
    /// This function is invoked by the analytics processor for each checkpoint.
    async fn process_checkpoint(
        &self,
        checkpoint_data: &Arc<CheckpointData>,
    ) -> Result<Box<dyn Iterator<Item = S> + Send + Sync>>
    where
        S: Send + Sync;
    /// Type of data being written by this processor i.e. checkpoint, object, etc
    fn file_type(&self) -> Result<FileType>;
    fn name(&self) -> &'static str;
}

/// Trait for processing transactions in parallel across all transactions in a checkpoint.
/// Implementations will extract and transform transaction data into structured rows for analytics.
#[async_trait]
pub trait TransactionProcessor<Row>: Send + Sync + 'static {
    /// Process a single transaction at the given index and return a boxed iterator over the rows.
    /// The implementation should handle extracting the transaction from the checkpoint.
    async fn process_transaction(
        &self,
        tx_idx: usize,
        checkpoint: &CheckpointData,
    ) -> Result<Box<dyn Iterator<Item = Row> + Send + Sync>>;
}

/// Run transaction processing in parallel across all transactions in a checkpoint.
pub async fn process_transactions<Row, P>(
    checkpoint: Arc<CheckpointData>,
    processor: Arc<P>,
) -> Result<Box<dyn Iterator<Item = Row> + Send + Sync>>
where
    Row: Send + Sync + 'static,
    P: TransactionProcessor<Row>,
{
    // Process transactions in parallel using buffered stream for ordered execution
    let txn_len = checkpoint.transactions.len();
    let mut entries_vec = Vec::with_capacity(txn_len);

    let mut stream = stream::iter(0..txn_len)
        .map(|idx| {
            let checkpoint = checkpoint.clone();
            let processor = processor.clone();
            tokio::spawn(async move { processor.process_transaction(idx, &checkpoint).await })
        })
        .buffered(*TRANSACTION_CONCURRENCY_LIMIT);

    while let Some(join_res) = stream.next().await {
        match join_res {
            Ok(Ok(tx_entries)) => {
                // Store the iterator for later flattening
                entries_vec.push(tx_entries);
            }
            Ok(Err(e)) => {
                // Task executed but application logic returned an error
                return Err(e);
            }
            Err(e) => {
                // Task panicked or was cancelled
                return Err(anyhow::anyhow!("Task join error: {}", e));
            }
        }
    }

    let flattened_iter = entries_vec.into_iter().flatten();
    Ok(Box::new(flattened_iter))
}

fn initial_shared_version(object: &Object) -> Option<u64> {
    match object.owner {
        Owner::Shared {
            initial_shared_version,
        } => Some(initial_shared_version.value()),
        _ => None,
    }
}

fn get_owner_type(object: &Object) -> OwnerType {
    match object.owner {
        Owner::AddressOwner(_) => OwnerType::AddressOwner,
        Owner::ObjectOwner(_) => OwnerType::ObjectOwner,
        Owner::Shared { .. } => OwnerType::Shared,
        Owner::Immutable => OwnerType::Immutable,
        Owner::ConsensusAddressOwner { .. } => OwnerType::AddressOwner,
    }
}

fn get_owner_address(object: &Object) -> Option<String> {
    match object.owner {
        Owner::AddressOwner(address) => Some(address.to_string()),
        Owner::ObjectOwner(address) => Some(address.to_string()),
        Owner::Shared { .. } => None,
        Owner::Immutable => None,
        Owner::ConsensusAddressOwner { owner, .. } => Some(owner.to_string()),
    }
}

fn get_is_consensus(object: &Object) -> bool {
    match object.owner {
        Owner::AddressOwner(_) => false,
        Owner::ObjectOwner(_) => false,
        Owner::Shared { .. } => true,
        Owner::Immutable => false,
        Owner::ConsensusAddressOwner { .. } => true,
    }
}

// Helper class to track input object kind.
// Build sets of object ids for input, shared input and gas coin objects as defined
// in the transaction data.
// Input objects include coins and shared.
struct InputObjectTracker {
    shared: BTreeSet<ObjectID>,
    coins: BTreeSet<ObjectID>,
    input: BTreeSet<ObjectID>,
}

impl InputObjectTracker {
    fn new(txn_data: &TransactionData) -> Self {
        let shared: BTreeSet<ObjectID> = txn_data
            .shared_input_objects()
            .iter()
            .map(|shared_io| shared_io.id())
            .collect();
        let coins: BTreeSet<ObjectID> = txn_data.gas().iter().map(|obj_ref| obj_ref.0).collect();
        let input: BTreeSet<ObjectID> = txn_data
            .input_objects()
            .expect("Input objects must be valid")
            .iter()
            .map(|io_kind| io_kind.object_id())
            .collect();
        Self {
            shared,
            coins,
            input,
        }
    }

    fn get_input_object_kind(&self, object_id: &ObjectID) -> Option<InputObjectKind> {
        if self.coins.contains(object_id) {
            Some(InputObjectKind::GasCoin)
        } else if self.shared.contains(object_id) {
            Some(InputObjectKind::SharedInput)
        } else if self.input.contains(object_id) {
            Some(InputObjectKind::Input)
        } else {
            None
        }
    }
}

// Helper class to track object status.
// Build sets of object ids for created, mutated and deleted objects as reported
// in the transaction effects.
struct ObjectStatusTracker {
    created: BTreeSet<ObjectID>,
    mutated: BTreeSet<ObjectID>,
    deleted: BTreeSet<ObjectID>,
}

impl ObjectStatusTracker {
    fn new(effects: &TransactionEffects) -> Self {
        let created: BTreeSet<ObjectID> = effects
            .created()
            .iter()
            .map(|(obj_ref, _)| obj_ref.0)
            .collect();
        let mutated: BTreeSet<ObjectID> = effects
            .mutated()
            .iter()
            .chain(effects.unwrapped().iter())
            .map(|(obj_ref, _)| obj_ref.0)
            .collect();
        let deleted: BTreeSet<ObjectID> = effects
            .all_tombstones()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        Self {
            created,
            mutated,
            deleted,
        }
    }

    fn get_object_status(&self, object_id: &ObjectID) -> Option<ObjectStatus> {
        if self.mutated.contains(object_id) {
            Some(ObjectStatus::Mutated)
        } else if self.deleted.contains(object_id) {
            Some(ObjectStatus::Deleted)
        } else if self.created.contains(object_id) {
            Some(ObjectStatus::Created)
        } else {
            None
        }
    }
}

async fn get_move_struct<T: PackageStore>(
    struct_tag: &StructTag,
    contents: &[u8],
    resolver: &Resolver<T>,
) -> Result<MoveStruct> {
    let move_struct = match resolver
        .type_layout(TypeTag::Struct(Box::new(struct_tag.clone())))
        .await?
    {
        MoveTypeLayout::Struct(move_struct_layout) => {
            BoundedVisitor::deserialize_struct(contents, &move_struct_layout)
        }
        _ => Err(anyhow!("Object is not a move struct")),
    }?;
    Ok(move_struct)
}

#[derive(Debug, Default)]
pub struct WrappedStruct {
    object_id: Option<ObjectID>,
    struct_tag: Option<StructTag>,
}

fn parse_struct(
    path: &str,
    move_struct: MoveStruct,
    all_structs: &mut BTreeMap<String, WrappedStruct>,
) {
    let mut wrapped_struct = WrappedStruct {
        struct_tag: Some(move_struct.type_),
        ..Default::default()
    };
    for (k, v) in move_struct.fields {
        parse_struct_field(
            &format!("{}.{}", path, &k),
            v,
            &mut wrapped_struct,
            all_structs,
        );
    }
    all_structs.insert(path.to_string(), wrapped_struct);
}

fn parse_struct_field(
    path: &str,
    move_value: MoveValue,
    curr_struct: &mut WrappedStruct,
    all_structs: &mut BTreeMap<String, WrappedStruct>,
) {
    match move_value {
        MoveValue::Struct(move_struct) => {
            let values = move_struct
                .fields
                .iter()
                .map(|(id, value)| (id.to_string(), value))
                .collect::<BTreeMap<_, _>>();
            let struct_name = format!(
                "0x{}::{}::{}",
                move_struct.type_.address.short_str_lossless(),
                move_struct.type_.module,
                move_struct.type_.name
            );
            if "0x2::object::UID" == struct_name {
                if let Some(MoveValue::Struct(id_struct)) = values.get("id").cloned() {
                    let id_values = id_struct
                        .fields
                        .iter()
                        .map(|(id, value)| (id.to_string(), value))
                        .collect::<BTreeMap<_, _>>();
                    if let Some(MoveValue::Address(address) | MoveValue::Signer(address)) =
                        id_values.get("bytes").cloned()
                    {
                        curr_struct.object_id = Some(ObjectID::from_address(*address))
                    }
                }
            } else if "0x1::option::Option" == struct_name {
                // Option in sui move is implemented as vector of size 1
                if let Some(MoveValue::Vector(vec_values)) = values.get("vec").cloned() {
                    if let Some(first_value) = vec_values.first() {
                        parse_struct_field(
                            &format!("{}[0]", path),
                            first_value.clone(),
                            curr_struct,
                            all_structs,
                        );
                    }
                }
            } else if !WRAPPED_INDEXING_DISALLOW_LIST.contains(&&*struct_name) {
                // Do not index most common struct types i.e. string, url, etc
                parse_struct(path, move_struct, all_structs)
            }
        }
        MoveValue::Variant(v) => {
            for (k, field) in v.fields.iter() {
                parse_struct_field(
                    &format!("{}.{}", path, k),
                    field.clone(),
                    curr_struct,
                    all_structs,
                );
            }
        }
        MoveValue::Vector(fields) => {
            for (index, field) in fields.iter().enumerate() {
                parse_struct_field(
                    &format!("{}[{}]", path, &index),
                    field.clone(),
                    curr_struct,
                    all_structs,
                );
            }
        }
        _ => {}
    }
}

pub async fn wait_for_cache(checkpoint_data: &CheckpointData, package_cache: &PackageCache) {
    let sequence_number = *checkpoint_data.checkpoint_summary.sequence_number();
    package_cache.coordinator.wait(sequence_number).await;
}

#[cfg(test)]
mod tests {
    use crate::handlers::parse_struct;
    use move_core_types::account_address::AccountAddress;
    use move_core_types::annotated_value::{MoveStruct, MoveValue, MoveVariant};
    use move_core_types::identifier::Identifier;
    use move_core_types::language_storage::StructTag;
    use std::collections::BTreeMap;
    use std::str::FromStr;
    use sui_types::base_types::ObjectID;

    #[tokio::test]
    async fn test_wrapped_object_parsing() -> anyhow::Result<()> {
        let uid_field = MoveValue::Struct(MoveStruct {
            type_: StructTag::from_str("0x2::object::UID")?,
            fields: vec![(
                Identifier::from_str("id")?,
                MoveValue::Struct(MoveStruct {
                    type_: StructTag::from_str("0x2::object::ID")?,
                    fields: vec![(
                        Identifier::from_str("bytes")?,
                        MoveValue::Signer(AccountAddress::from_hex_literal("0x300")?),
                    )],
                }),
            )],
        });
        let balance_field = MoveValue::Struct(MoveStruct {
            type_: StructTag::from_str("0x2::balance::Balance")?,
            fields: vec![(Identifier::from_str("value")?, MoveValue::U32(10))],
        });
        let move_struct = MoveStruct {
            type_: StructTag::from_str("0x2::test::Test")?,
            fields: vec![
                (Identifier::from_str("id")?, uid_field),
                (Identifier::from_str("principal")?, balance_field),
            ],
        };
        let mut all_structs = BTreeMap::new();
        parse_struct("$", move_struct, &mut all_structs);
        assert_eq!(
            all_structs.get("$").unwrap().object_id,
            Some(ObjectID::from_hex_literal("0x300")?)
        );
        assert_eq!(
            all_structs.get("$.principal").unwrap().struct_tag,
            Some(StructTag::from_str("0x2::balance::Balance")?)
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_wrapped_object_parsing_within_enum() -> anyhow::Result<()> {
        let uid_field = MoveValue::Struct(MoveStruct {
            type_: StructTag::from_str("0x2::object::UID")?,
            fields: vec![(
                Identifier::from_str("id")?,
                MoveValue::Struct(MoveStruct {
                    type_: StructTag::from_str("0x2::object::ID")?,
                    fields: vec![(
                        Identifier::from_str("bytes")?,
                        MoveValue::Signer(AccountAddress::from_hex_literal("0x300")?),
                    )],
                }),
            )],
        });
        let balance_field = MoveValue::Struct(MoveStruct {
            type_: StructTag::from_str("0x2::balance::Balance")?,
            fields: vec![(Identifier::from_str("value")?, MoveValue::U32(10))],
        });
        let move_enum = MoveVariant {
            type_: StructTag::from_str("0x2::test::TestEnum")?,
            variant_name: Identifier::from_str("TestVariant")?,
            tag: 0,
            fields: vec![
                (Identifier::from_str("field0")?, MoveValue::U64(10)),
                (Identifier::from_str("principal")?, balance_field),
            ],
        };
        let move_struct = MoveStruct {
            type_: StructTag::from_str("0x2::test::Test")?,
            fields: vec![
                (Identifier::from_str("id")?, uid_field),
                (
                    Identifier::from_str("enum_field")?,
                    MoveValue::Variant(move_enum),
                ),
            ],
        };
        let mut all_structs = BTreeMap::new();
        parse_struct("$", move_struct, &mut all_structs);
        assert_eq!(
            all_structs.get("$").unwrap().object_id,
            Some(ObjectID::from_hex_literal("0x300")?)
        );
        assert_eq!(
            all_structs
                .get("$.enum_field.principal")
                .unwrap()
                .struct_tag,
            Some(StructTag::from_str("0x2::balance::Balance")?)
        );
        Ok(())
    }
}
