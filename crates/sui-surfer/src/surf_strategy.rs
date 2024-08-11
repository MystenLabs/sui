// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use move_binary_format::normalized::Type;
use move_core_types::language_storage::StructTag;
use rand::{seq::SliceRandom, Rng};
use sui_types::{
    base_types::ObjectRef,
    transaction::{CallArg, ObjectArg},
};
use tokio::time::Instant;
use tracing::debug;

use crate::surfer_state::{EntryFunction, SurferState};

enum InputObjectPassKind {
    Value,
    ByRef,
    MutRef,
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
        let mut failed = false;
        for param in params {
            let arg = match param {
                Type::Bool => CallArg::Pure(bcs::to_bytes(&state.rng.gen::<bool>()).unwrap()),
                Type::U8 => CallArg::Pure(bcs::to_bytes(&state.rng.gen::<u8>()).unwrap()),
                Type::U16 => CallArg::Pure(bcs::to_bytes(&state.rng.gen::<u16>()).unwrap()),
                Type::U32 => CallArg::Pure(bcs::to_bytes(&state.rng.gen::<u32>()).unwrap()),
                Type::U64 => CallArg::Pure(bcs::to_bytes(&state.rng.gen::<u64>()).unwrap()),
                Type::U128 => CallArg::Pure(bcs::to_bytes(&state.rng.gen::<u128>()).unwrap()),
                Type::Address => CallArg::Pure(
                    bcs::to_bytes(&state.cluster.get_addresses().choose(&mut state.rng)).unwrap(),
                ),
                ty @ Type::Struct { .. } => {
                    match Self::choose_object_call_arg(
                        state,
                        InputObjectPassKind::Value,
                        ty,
                        &mut chosen_owned_objects,
                    )
                    .await
                    {
                        Some(arg) => arg,
                        None => {
                            failed = true;
                            break;
                        }
                    }
                }
                Type::Reference(ty) => {
                    match Self::choose_object_call_arg(
                        state,
                        InputObjectPassKind::ByRef,
                        *ty,
                        &mut chosen_owned_objects,
                    )
                    .await
                    {
                        Some(arg) => arg,
                        None => {
                            failed = true;
                            break;
                        }
                    }
                }
                Type::MutableReference(ty) => {
                    match Self::choose_object_call_arg(
                        state,
                        InputObjectPassKind::MutRef,
                        *ty,
                        &mut chosen_owned_objects,
                    )
                    .await
                    {
                        Some(arg) => arg,
                        None => {
                            failed = true;
                            break;
                        }
                    }
                }
                Type::U256 | Type::Signer | Type::Vector(_) | Type::TypeParameter(_) => {
                    failed = true;
                    break;
                }
            };
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
            None
        } else {
            Some(args)
        }
    }

    async fn choose_object_call_arg(
        state: &mut SurferState,
        kind: InputObjectPassKind,
        arg_type: Type,
        chosen_owned_objects: &mut Vec<(StructTag, ObjectRef)>,
    ) -> Option<CallArg> {
        let type_tag = match arg_type {
            Type::Struct {
                address,
                module,
                name,
                type_arguments,
            } => StructTag {
                address,
                module,
                name,
                type_params: type_arguments
                    .into_iter()
                    .map(|t| t.into_type_tag().unwrap())
                    .collect(),
            },
            _ => {
                return None;
            }
        };
        let owned = state.matching_owned_objects_count(&type_tag);
        let shared = state.matching_shared_objects_count(&type_tag).await;
        let immutable = state.matching_immutable_objects_count(&type_tag).await;

        let total_matching_count = match kind {
            InputObjectPassKind::Value => owned,
            InputObjectPassKind::MutRef => owned + shared,
            InputObjectPassKind::ByRef => owned + shared + immutable,
        };
        if total_matching_count == 0 {
            return None;
        }
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
                mutable: matches!(kind, InputObjectPassKind::MutRef),
            }));
        }
        n -= shared;
        let obj_ref = state.choose_nth_immutable_object(&type_tag, n).await;
        Some(CallArg::Object(ObjectArg::ImmOrOwnedObject(obj_ref)))
    }
}
