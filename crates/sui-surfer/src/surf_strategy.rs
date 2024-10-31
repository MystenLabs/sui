// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, Mutex},
    time::Duration,
};

use futures::{future::Either, Future};
use move_binary_format::normalized::Type;
use move_core_types::language_storage::StructTag;
use rand::{seq::SliceRandom, Rng};
use sui_types::{
    base_types::ObjectRef,
    transaction::{CallArg, ObjectArg},
};
use tokio::time::Instant;
use tracing::{error, info, instrument, warn};

use crate::surfer_state::{get_type_tag, EntryFunction, SurferState};

enum InputObjectPassKind {
    Value,
    ByRef,
    MutRef,
}

pub enum RewriteResponse {
    Call,
    Skip,             // Skip this call attempt
    ForgetEntryPoint, // Forget the entry point
}

type CallRewriteFn = dyn Fn(&SurferState, &EntryFunction, &mut Vec<CallArg>) -> RewriteResponse
    + Send
    + Sync
    + 'static;

#[derive(Default, Debug, Clone, Copy)]
pub enum ErrorChecks {
    /// No error checking at all
    #[default]
    None,
    /// Transactions must be well-formed, but can abort at execution time.
    WellFormed,
    /// Transactions must exit with a success status.
    Strict,
}

#[derive(Debug, Clone)]
pub enum ExitCondition {
    Timeout(Duration),
    AllEntryPointsCalledSuccessfully,
}

impl Default for ExitCondition {
    fn default() -> Self {
        Self::Timeout(Duration::from_secs(60))
    }
}

#[derive(Clone, Default)]
pub struct SurfStrategy {
    min_tx_interval: Duration,

    exit_condition: ExitCondition,

    error_checking_mode: ErrorChecks,

    // Function call helpers, which, given a function name (specified as 'module::func'),
    // can re-write the arguments after sui surfer has chosen them. Can return false to
    // indicate that the call should not be attempted.
    call_rewriters: HashMap<String, Arc<CallRewriteFn>>,
}

impl SurfStrategy {
    pub fn new(min_tx_interval: Duration) -> Self {
        Self {
            min_tx_interval,
            exit_condition: ExitCondition::Timeout(Duration::from_secs(60)),
            error_checking_mode: ErrorChecks::None,
            call_rewriters: HashMap::new(),
        }
    }

    pub fn set_exit_condition(&mut self, condition: ExitCondition) {
        self.exit_condition = condition;
    }

    pub fn set_error_checking_mode(&mut self, mode: ErrorChecks) {
        self.error_checking_mode = mode;
    }

    pub fn add_call_rewriter(&mut self, function_name: &[&str], rewriter: Arc<CallRewriteFn>) {
        for function_name in function_name {
            self.call_rewriters
                .insert(function_name.to_string(), rewriter.clone());
        }
    }

    pub fn finished(&self, state: &SurferState) -> impl Future<Output = ()> + 'static {
        match self.exit_condition {
            ExitCondition::Timeout(duration) => Either::Left(tokio::time::sleep(duration)),
            ExitCondition::AllEntryPointsCalledSuccessfully => {
                let mut stats_rx = state.stats.subscribe();
                let remaining_entry_functions: Arc<Mutex<BTreeSet<_>>> = Arc::new(Mutex::new(
                    state.entry_functions.read().iter().cloned().collect(),
                ));

                Either::Right(async move {
                    // If we exit early (due to a timeout higher up the stack) log the missing functions
                    let _guard = scopeguard::guard((), |_| {
                        let remaining_entry_functions = remaining_entry_functions.lock().unwrap();
                        if remaining_entry_functions.len() > 0 {
                            error!(
                                "The following entry functions were not called: {:?}",
                                remaining_entry_functions
                                    .iter()
                                    .map(|f| f.qualified_name())
                                    .collect::<Vec<_>>()
                            );
                        }
                    });

                    loop {
                        stats_rx.changed().await.unwrap();
                        let stats = stats_rx.borrow_and_update();
                        let mut remaining_entry_functions =
                            remaining_entry_functions.lock().unwrap();
                        remaining_entry_functions.retain(|func| {
                            let key = (func.package, func.module.clone(), func.function.clone());
                            !stats.unique_move_functions_called_success.contains(&key)
                        });
                        if remaining_entry_functions.is_empty() {
                            break;
                        }
                    }
                })
            }
        }
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
        assert!(!entry_functions.is_empty());

        entry_functions.shuffle(&mut state.rng);
        let mut entry_functions_to_remove = BTreeSet::new();

        for entry in entry_functions {
            let next_tx_time = Instant::now() + self.min_tx_interval;
            let Some(mut args) = Self::choose_function_call_args(state, &entry).await else {
                warn!(
                    "Failed to choose arguments for Move function {:?}::{:?}",
                    entry.module, entry.function
                );
                continue;
            };

            let name = entry.qualified_name();
            if let Some(helper) = self.call_rewriters.get(&name) {
                let resp = helper(state, &entry, &mut args);
                if matches!(resp, RewriteResponse::ForgetEntryPoint) {
                    entry_functions_to_remove.insert(name.clone());
                }

                match resp {
                    RewriteResponse::Call => (),
                    RewriteResponse::Skip | RewriteResponse::ForgetEntryPoint => {
                        // return the owned objects to the pool
                        for (arg, param) in args.into_iter().zip(entry.parameters.iter()) {
                            if let CallArg::Object(ObjectArg::ImmOrOwnedObject(obj_ref)) = arg {
                                state
                                    .owned_objects
                                    .get_mut(&get_type_tag(param.clone()).unwrap())
                                    .unwrap()
                                    .insert(obj_ref);
                            }
                        }
                        continue;
                    }
                }
            }

            state
                .execute_move_transaction(&entry, args, self.error_checking_mode)
                .await;
            tokio::time::sleep_until(next_tx_time).await;
        }

        info!("removing entry functions: {:?}", entry_functions_to_remove);
        state
            .entry_functions
            .write()
            .retain(|entry| !entry_functions_to_remove.contains(&entry.qualified_name()));

        tokio::time::sleep(self.min_tx_interval).await;
    }

    #[instrument(skip_all, fields(state = %state.id, function = %entry.qualified_name()))]
    async fn choose_function_call_args(
        state: &mut SurferState,
        entry: &EntryFunction,
    ) -> Option<Vec<CallArg>> {
        let params = entry.parameters.clone();

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
        let type_tag = get_type_tag(arg_type)?;

        let owned = state.matching_owned_objects_count(&type_tag);
        let shared = state.matching_shared_objects_count(&type_tag).await;
        let immutable = state.matching_immutable_objects_count(&type_tag).await;

        info!(
            "type_tag: {}, Owned: {}, Shared: {}, Immutable: {}",
            type_tag, owned, shared, immutable
        );

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
