// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use move_binary_format::normalized;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::StructTag;
use rand::{Rng, RngCore, rngs::StdRng, seq::SliceRandom};
use sui_types::{
    SUI_FRAMEWORK_ADDRESS,
    base_types::{ObjectID, ObjectRef, SuiAddress},
    transaction::{CallArg, ObjectArg, SharedObjectMutability},
};
use tokio::time::Instant;
use tracing::debug;

use crate::surfer_state::{EntryFunction, SurferState};

type Datatype = normalized::Datatype<normalized::ArcIdentifier>;

/// If `dt` is a `sui::transfer::Receiving<T>`, returns the inner type `T`.
fn receiving_inner_type(dt: &Datatype) -> Option<&Type> {
    if dt.module.address == SUI_FRAMEWORK_ADDRESS
        && dt.module.name.as_ident_str() == IdentStr::new("transfer").unwrap()
        && dt.name.as_ident_str() == IdentStr::new("Receiving").unwrap()
        && dt.type_arguments.len() == 1
    {
        Some(&dt.type_arguments[0])
    } else {
        None
    }
}

enum InputObjectPassKind {
    Value,
    ByRef,
    MutRef,
}

type Type = normalized::Type<normalized::ArcIdentifier>;

/// Upper bound on the length of randomly generated `vector<T>` pure arguments.
/// Kept small so transactions stay cheap while still exercising non-empty vectors.
const MAX_VECTOR_LEN: usize = 16;

/// BCS-encode a randomly generated value for a "pure" (non-object) Move type.
/// Returns `None` for types that cannot be passed as a `CallArg::Pure` (objects,
/// type parameters, signer, references). Vectors of pure element types are
/// supported recursively.
fn random_pure_arg_bytes(rng: &mut StdRng, ty: &Type) -> Option<Vec<u8>> {
    Some(match ty {
        Type::Bool => vec![u8::from(rng.r#gen::<bool>())],
        Type::U8 => vec![rng.r#gen::<u8>()],
        Type::U16 => rng.r#gen::<u16>().to_le_bytes().to_vec(),
        Type::U32 => rng.r#gen::<u32>().to_le_bytes().to_vec(),
        Type::U64 => rng.r#gen::<u64>().to_le_bytes().to_vec(),
        Type::U128 => rng.r#gen::<u128>().to_le_bytes().to_vec(),
        // BCS encodes u256 and address as 32 little-endian bytes; any 32 bytes is valid.
        Type::U256 | Type::Address => {
            let mut bytes = [0u8; 32];
            rng.fill_bytes(&mut bytes);
            bytes.to_vec()
        }
        Type::Vector(inner) => {
            let len = rng.gen_range(0..MAX_VECTOR_LEN);
            let mut out = uleb128_encode(len);
            for _ in 0..len {
                out.extend(random_pure_arg_bytes(rng, inner)?);
            }
            out
        }
        _ => return None,
    })
}

/// ULEB128 encoding used by BCS for sequence length prefixes.
fn uleb128_encode(mut value: usize) -> Vec<u8> {
    let mut out = vec![];
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
    out
}

#[derive(Clone, Default)]
pub struct SurfStrategy {
    min_tx_interval: Duration,
}

impl SurfStrategy {
    pub fn new(min_tx_interval: Duration) -> Self {
        Self { min_tx_interval }
    }

    /// Given a state and a list of callable Move entry functions,
    /// explore them for a while, and eventually return. This function may
    /// not return in some situations, so its important to call it with a
    /// timeout or select! to ensure the task doesn't block forever.
    pub async fn surf_for_a_while(
        &mut self,
        state: &mut SurferState,
        mut entry_functions: Vec<EntryFunction>,
    ) {
        entry_functions.shuffle(&mut state.rng);
        for entry in entry_functions {
            let next_tx_time = Instant::now() + self.min_tx_interval;
            let Some(args) = Self::choose_function_call_args(state, entry.parameters).await else {
                debug!(
                    "Failed to choose arguments for Move function {:?}::{:?}",
                    entry.module, entry.function
                );
                continue;
            };
            state
                .execute_move_transaction(entry.package, entry.module, entry.function, args)
                .await;
            tokio::time::sleep_until(next_tx_time).await;
        }
    }

    async fn choose_function_call_args(
        state: &mut SurferState,
        params: Vec<Type>,
    ) -> Option<Vec<CallArg>> {
        let mut args = vec![];
        let mut chosen_owned_objects = vec![];
        // Object ids chosen so far, used to coordinate `Receiving<T>` arguments
        // with the parent object they were transferred to.
        let mut chosen_object_ids: Vec<ObjectID> = vec![];
        // Receivable objects taken from inventory, to restore if assembly fails.
        let mut chosen_received: Vec<(SuiAddress, StructTag, ObjectRef)> = vec![];
        let mut failed = false;
        for param in params {
            let arg = match param {
                Type::Bool => CallArg::Pure(bcs::to_bytes(&state.rng.r#gen::<bool>()).unwrap()),
                Type::U8 => CallArg::Pure(bcs::to_bytes(&state.rng.r#gen::<u8>()).unwrap()),
                Type::U16 => CallArg::Pure(bcs::to_bytes(&state.rng.r#gen::<u16>()).unwrap()),
                Type::U32 => CallArg::Pure(bcs::to_bytes(&state.rng.r#gen::<u32>()).unwrap()),
                Type::U64 => CallArg::Pure(bcs::to_bytes(&state.rng.r#gen::<u64>()).unwrap()),
                Type::U128 => CallArg::Pure(bcs::to_bytes(&state.rng.r#gen::<u128>()).unwrap()),
                Type::Address => CallArg::Pure(
                    bcs::to_bytes(&state.cluster.get_addresses().choose(&mut state.rng)).unwrap(),
                ),
                ty @ Type::Datatype(_) => {
                    // A `Receiving<T>` argument is handled specially: it references a
                    // child object transferred to one of the parent objects already
                    // chosen for this call.
                    let receiving = match &ty {
                        Type::Datatype(dt) => receiving_inner_type(dt).cloned(),
                        _ => None,
                    };
                    let chosen = if let Some(inner) = receiving {
                        Self::choose_receiving_arg(
                            state,
                            &inner,
                            &chosen_object_ids,
                            &mut chosen_received,
                        )
                        .await
                    } else {
                        Self::choose_object_call_arg(
                            state,
                            InputObjectPassKind::Value,
                            ty,
                            &mut chosen_owned_objects,
                        )
                        .await
                    };
                    match chosen {
                        Some(arg) => arg,
                        None => {
                            failed = true;
                            break;
                        }
                    }
                }
                Type::Reference(mut_, ty) => {
                    let kind = if mut_ {
                        InputObjectPassKind::MutRef
                    } else {
                        InputObjectPassKind::ByRef
                    };
                    match Self::choose_object_call_arg(state, kind, *ty, &mut chosen_owned_objects)
                        .await
                    {
                        Some(arg) => arg,
                        None => {
                            failed = true;
                            break;
                        }
                    }
                }
                ref t @ (Type::U256 | Type::Vector(_)) => {
                    match random_pure_arg_bytes(&mut state.rng, t) {
                        Some(bytes) => CallArg::Pure(bytes),
                        None => {
                            // e.g. vector<Object>, which can't be a pure argument.
                            failed = true;
                            break;
                        }
                    }
                }
                Type::Signer | Type::TypeParameter(_) => {
                    failed = true;
                    break;
                }
            };
            if let CallArg::Object(obj_arg) = &arg {
                chosen_object_ids.push(obj_arg.id());
            }
            args.push(arg);
        }
        if failed {
            for (struct_tag, obj_ref) in chosen_owned_objects {
                state
                    .owned_objects
                    .get_mut(&struct_tag)
                    .unwrap()
                    .insert(obj_ref);
            }
            for (parent, struct_tag, obj_ref) in chosen_received {
                state
                    .return_receivable_object(parent, struct_tag, obj_ref)
                    .await;
            }
            None
        } else {
            Some(args)
        }
    }

    /// Choose a `Receiving<T>` argument: find a child object of type `T` that was
    /// transferred to one of the already-chosen parent objects' addresses.
    async fn choose_receiving_arg(
        state: &mut SurferState,
        inner: &Type,
        chosen_object_ids: &[ObjectID],
        chosen_received: &mut Vec<(SuiAddress, StructTag, ObjectRef)>,
    ) -> Option<CallArg> {
        let pool = state.pool.read().await;
        let type_tag = match inner {
            Type::Datatype(dt) => dt.to_struct_tag(&*pool),
            _ => return None,
        };
        drop(pool);
        for parent_id in chosen_object_ids {
            let parent_addr = SuiAddress::from(*parent_id);
            if let Some(obj_ref) = state.take_receivable_object(&parent_addr, &type_tag).await {
                chosen_received.push((parent_addr, type_tag.clone(), obj_ref));
                return Some(CallArg::Object(ObjectArg::Receiving(obj_ref)));
            }
        }
        None
    }

    async fn choose_object_call_arg(
        state: &mut SurferState,
        kind: InputObjectPassKind,
        arg_type: Type,
        chosen_owned_objects: &mut Vec<(StructTag, ObjectRef)>,
    ) -> Option<CallArg> {
        let pool = state.pool.read().await;
        let type_tag = match arg_type {
            Type::Datatype(dt) => dt.to_struct_tag(&*pool),
            _ => {
                return None;
            }
        };
        drop(pool);
        let owned = state.matching_owned_objects_count(&type_tag);
        let shared = state.matching_shared_objects_count(&type_tag).await;
        let party = state.matching_party_objects_count(&type_tag);
        let immutable = state.matching_immutable_objects_count(&type_tag).await;

        // Shared and party objects can be consumed by-value (e.g. shared object
        // deletion / party transfer) as well as passed by reference. Immutable
        // objects are only valid as immutable references.
        let total_matching_count = match kind {
            InputObjectPassKind::Value | InputObjectPassKind::MutRef => owned + shared + party,
            InputObjectPassKind::ByRef => owned + shared + party + immutable,
        };
        if total_matching_count == 0 {
            return None;
        }
        let mutable = matches!(
            kind,
            InputObjectPassKind::MutRef | InputObjectPassKind::Value
        );
        let consensus_mutability = if mutable {
            SharedObjectMutability::Mutable
        } else {
            SharedObjectMutability::Immutable
        };
        let mut n = state.rng.gen_range(0..total_matching_count);
        if n < owned {
            let obj_ref = state.choose_nth_owned_object(&type_tag, n);
            chosen_owned_objects.push((type_tag, obj_ref));
            return Some(CallArg::Object(ObjectArg::ImmOrOwnedObject(obj_ref)));
        }
        n -= owned;
        if n < shared {
            let (id, initial_shared_version) = state.choose_nth_shared_object(&type_tag, n).await;
            return Some(CallArg::Object(ObjectArg::SharedObject {
                id,
                initial_shared_version,
                mutability: consensus_mutability,
            }));
        }
        n -= shared;
        if n < party {
            // Party objects are referenced like shared objects, using their current
            // consensus start version as the initial shared version.
            let (id, initial_shared_version) = state.choose_nth_party_object(&type_tag, n);
            return Some(CallArg::Object(ObjectArg::SharedObject {
                id,
                initial_shared_version,
                mutability: consensus_mutability,
            }));
        }
        n -= party;
        let obj_ref = state.choose_nth_immutable_object(&type_tag, n).await;
        Some(CallArg::Object(ObjectArg::ImmOrOwnedObject(obj_ref)))
    }
}
