// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_common::assert_reachable;
use mysten_common::random::get_rng;
use rand::Rng;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber};
use sui_types::object::Owner;
use sui_types::transaction::{CallArg, ObjectArg};
use tracing::debug;

use super::lifecycle::{
    Lifecycle, LifecycleCall, LifecycleStep, StepContext, find_object, pure, shared_mut,
};
use super::operations::{
    PARTY_CREATE, PARTY_END_STREAM, PARTY_HAND_OFF, PARTY_HOLDER_READ, PARTY_MUTATE,
    PARTY_OLD_STREAM, PARTY_OWNED_MUTATE, PARTY_RACING_MUTATE, PARTY_RESTART_STREAM,
    PARTY_RETRANSFER, PARTY_UNWRAP, PARTY_WRAP,
};
use crate::workloads::payload::BatchedTransactionStatus;

#[derive(Clone, Copy, Debug)]
pub(super) enum PartyStep {
    /// Create a party object.
    Create,
    /// Mutate a party object on its current stream.
    Mutate,
    /// Mutate a party object on its current stream, submitted alongside the tx that ends the
    /// stream. It may be sequenced before or after the end tx.
    RacingMutate,
    /// End a party object's stream by transferring it to an address.
    EndStream,
    /// Mutate a party object using a start version whose stream has ended (expected to fail).
    OldStream,
    /// Mutate the address-owned incarnation of a formerly-party object.
    OwnedMutate,
    /// Start a new stream by transferring an address-owned object back to a party owner.
    RestartStream,
    /// Transfer a party object by value to its current owner, which keeps its stream.
    Retransfer,
    /// End a party object's stream by transferring it to the partner as a party owner.
    HandOff,
    /// End a party object's stream by wrapping it in an address-owned `Holder`.
    Wrap,
    /// Unwrap a party object from its `Holder` to a party owner or an address.
    Unwrap,
    /// Read a `Holder`, submitted alongside the unwrap tx that consumes it.
    HolderRead,
}

/// A payload's party-object lifecycle. Each acting batch advances it one step:
/// `NeedCreate -> Live -> (StreamEnded -> Owned -> (Live | NeedCreate) | HandedOff -> NeedCreate
/// | Wrapped -> (Live | StreamEnded))`.
/// The steps after `Live` submit txs that reference the ended stream together with txs that
/// use the same object id under its current ownership. Their versions can collide, which
/// exercises the check that an ended-stream input waits for its stream-ended marker.
#[derive(Debug, Clone)]
enum PartyPhase {
    /// No party object is tracked; the next step creates one.
    NeedCreate,
    /// The object is on the consensus stream that began at `start_version`.
    Live {
        id: ObjectID,
        start_version: SequenceNumber,
        mutations: u32,
    },
    /// The stream that began at `ended_start_version` has ended and the object is
    /// address-owned at `addr_ref`. The next step mutates it through both the ended stream
    /// and its address-owned ref.
    StreamEnded {
        id: ObjectID,
        ended_start_version: SequenceNumber,
        addr_ref: ObjectRef,
    },
    /// The object is address-owned after its stream ended, and `rounds` steps have run in
    /// this phase. The next step sends txs on the ended stream, sometimes with another owned
    /// mutation, or it restarts a stream on the same id, or it abandons the object.
    Owned {
        id: ObjectID,
        ended_start_version: SequenceNumber,
        addr_ref: ObjectRef,
        rounds: u32,
    },
    /// The object moved to the partner on a new stream. The next step references the ended
    /// stream and then abandons the object.
    HandedOff {
        id: ObjectID,
        ended_start_version: SequenceNumber,
    },
    /// The object is wrapped in the address-owned `holder_ref`. The next step unwraps it to a
    /// new stream or to an address, together with txs on the ended stream.
    Wrapped {
        id: ObjectID,
        ended_start_version: SequenceNumber,
        holder_ref: ObjectRef,
    },
}

#[derive(Debug)]
pub(super) struct PartyLifecycle {
    start_probability: f64,
    phase: PartyPhase,
}

impl PartyLifecycle {
    /// The stream ends after at most this many mutations so the lifecycle keeps progressing.
    const MAX_MUTATIONS: u32 = 3;
    /// The object leaves the `Owned` phase after at most this many rounds.
    const MAX_OWNED_ROUNDS: u32 = 6;

    pub fn new(start_probability: f64) -> Self {
        Self {
            start_probability,
            phase: PartyPhase::NeedCreate,
        }
    }

    fn call(
        &self,
        function: &'static str,
        args: Vec<CallArg>,
        op: &'static str,
        step: PartyStep,
    ) -> LifecycleCall {
        let call =
            LifecycleCall::move_call("party_obj", function, args, op, LifecycleStep::Party(step));
        // Txs after a stream ends carry no sampled ops. Sampled ops would raise the owned
        // object's versions past the ended stream's assigned versions, and they would never
        // collide. The wrap tx carries none so that the holder is the only object it creates.
        // Sampled ops reject or abort most creates, which would delay the lifecycle.
        if matches!(
            step,
            PartyStep::Create
                | PartyStep::OldStream
                | PartyStep::OwnedMutate
                | PartyStep::RestartStream
                | PartyStep::Wrap
                | PartyStep::Unwrap
                | PartyStep::HolderRead
        ) {
            call.bare()
        } else {
            call
        }
    }

    fn set_value(
        &self,
        obj: CallArg,
        value: u64,
        op: &'static str,
        step: PartyStep,
    ) -> LifecycleCall {
        self.call("set_value", vec![obj, pure(&value)], op, step)
    }
}

impl Lifecycle for PartyLifecycle {
    fn max_step_calls(&self) -> usize {
        match self.phase {
            PartyPhase::NeedCreate => 1,
            PartyPhase::HandedOff { .. } => 2,
            PartyPhase::Live { .. }
            | PartyPhase::StreamEnded { .. }
            | PartyPhase::Owned { .. }
            | PartyPhase::Wrapped { .. } => 3,
        }
    }

    fn in_progress(&self) -> bool {
        !matches!(self.phase, PartyPhase::NeedCreate)
    }

    fn urgent(&self) -> bool {
        !matches!(self.phase, PartyPhase::NeedCreate | PartyPhase::Live { .. })
    }

    fn start_probability(&self) -> f64 {
        self.start_probability
    }

    fn next_step(&mut self, ctx: &StepContext) -> Vec<LifecycleCall> {
        let mut rng = get_rng();
        if let PartyPhase::Owned { rounds, .. } = self.phase
            && (rounds >= Self::MAX_OWNED_ROUNDS || rng.gen_bool(0.3))
            && rng.gen_bool(0.5)
        {
            // Abandon the address-owned object and start over with a new one.
            self.phase = PartyPhase::NeedCreate;
        }

        let sender_arg = pure(&ctx.sender);
        let partner_arg = pure(&ctx.partner);

        match self.phase.clone() {
            PartyPhase::NeedCreate => vec![self.call(
                "create",
                vec![pure(&0u64), sender_arg],
                PARTY_CREATE,
                PartyStep::Create,
            )],
            PartyPhase::Live {
                id,
                start_version,
                mutations,
            } => {
                let obj = shared_mut(id, start_version);
                if mutations >= Self::MAX_MUTATIONS || rng.gen_bool(0.5) {
                    // A single payload is its own partner, and a transfer to itself would
                    // not end the stream.
                    // Half of the ends go to an address, because only that end is followed by
                    // an owned mutation of the same id.
                    let end = match rng.gen_range(0..4) {
                        0 if ctx.partner != ctx.sender => self.call(
                            "to_party",
                            vec![obj.clone(), partner_arg],
                            PARTY_HAND_OFF,
                            PartyStep::HandOff,
                        ),
                        1 => self.call(
                            "wrap",
                            vec![obj.clone(), sender_arg],
                            PARTY_WRAP,
                            PartyStep::Wrap,
                        ),
                        _ => self.call(
                            "to_address",
                            vec![obj.clone(), sender_arg],
                            PARTY_END_STREAM,
                            PartyStep::EndStream,
                        ),
                    };
                    let mut calls = vec![end];
                    if rng.gen_bool(0.5) {
                        // A mutation sequenced after the end tx executes against the ended
                        // stream.
                        for value in [1, 2] {
                            calls.push(self.set_value(
                                obj.clone(),
                                value,
                                PARTY_RACING_MUTATE,
                                PartyStep::RacingMutate,
                            ));
                        }
                    }
                    calls
                } else if rng.gen_bool(0.5) {
                    vec![self.set_value(obj, 7, PARTY_MUTATE, PartyStep::Mutate)]
                } else {
                    vec![self.call(
                        "to_party",
                        vec![obj, sender_arg],
                        PARTY_RETRANSFER,
                        PartyStep::Retransfer,
                    )]
                }
            }
            PartyPhase::HandedOff {
                id,
                ended_start_version,
            } => {
                // The partner owns the object, so this payload can only use the ended stream.
                self.phase = PartyPhase::NeedCreate;
                let old_stream = shared_mut(id, ended_start_version);
                vec![
                    self.set_value(
                        old_stream.clone(),
                        5,
                        PARTY_OLD_STREAM,
                        PartyStep::OldStream,
                    ),
                    self.set_value(old_stream, 6, PARTY_OLD_STREAM, PartyStep::OldStream),
                ]
            }
            PartyPhase::Wrapped {
                id,
                ended_start_version,
                holder_ref,
            } => {
                let holder = CallArg::Object(ObjectArg::ImmOrOwnedObject(holder_ref));
                let unwrap = if rng.gen_bool(0.5) {
                    "unwrap_to_party"
                } else {
                    "unwrap_to_address"
                };
                let old_stream = shared_mut(id, ended_start_version);
                let mut calls = vec![
                    self.call(
                        unwrap,
                        vec![holder.clone(), sender_arg],
                        PARTY_UNWRAP,
                        PartyStep::Unwrap,
                    ),
                    self.set_value(
                        old_stream.clone(),
                        8,
                        PARTY_OLD_STREAM,
                        PartyStep::OldStream,
                    ),
                ];
                if rng.gen_bool(0.5) {
                    // Both txs use the holder at the same version, so at most one executes.
                    calls.push(self.call(
                        "holder_value",
                        vec![holder],
                        PARTY_HOLDER_READ,
                        PartyStep::HolderRead,
                    ));
                } else {
                    calls.push(self.set_value(
                        old_stream,
                        9,
                        PARTY_OLD_STREAM,
                        PartyStep::OldStream,
                    ));
                }
                calls
            }
            PartyPhase::StreamEnded {
                id,
                ended_start_version,
                addr_ref,
            } => {
                let old_stream = shared_mut(id, ended_start_version);
                vec![
                    self.set_value(
                        old_stream.clone(),
                        1,
                        PARTY_OLD_STREAM,
                        PartyStep::OldStream,
                    ),
                    self.set_value(old_stream, 2, PARTY_OLD_STREAM, PartyStep::OldStream),
                    self.set_value(
                        CallArg::Object(ObjectArg::ImmOrOwnedObject(addr_ref)),
                        3,
                        PARTY_OWNED_MUTATE,
                        PartyStep::OwnedMutate,
                    ),
                ]
            }
            PartyPhase::Owned {
                id,
                ended_start_version,
                addr_ref,
                rounds,
            } => {
                let old_stream = shared_mut(id, ended_start_version);
                if rounds < Self::MAX_OWNED_ROUNDS && rng.gen_bool(0.8) {
                    // Txs on the ended stream keep arriving in later batches, after owned
                    // mutations of the same id, so their assigned versions can match an owned
                    // version.
                    let mut calls = vec![
                        self.set_value(
                            old_stream.clone(),
                            1,
                            PARTY_OLD_STREAM,
                            PartyStep::OldStream,
                        ),
                        self.set_value(old_stream, 2, PARTY_OLD_STREAM, PartyStep::OldStream),
                    ];
                    if rng.gen_bool(0.5) {
                        // The owned mutation's result advances `rounds`.
                        calls.push(self.set_value(
                            CallArg::Object(ObjectArg::ImmOrOwnedObject(addr_ref)),
                            10 + rounds as u64,
                            PARTY_OWNED_MUTATE,
                            PartyStep::OwnedMutate,
                        ));
                    } else {
                        self.phase = PartyPhase::Owned {
                            id,
                            ended_start_version,
                            addr_ref,
                            rounds: rounds + 1,
                        };
                    }
                    return calls;
                }
                vec![
                    self.call(
                        "to_party",
                        vec![
                            CallArg::Object(ObjectArg::ImmOrOwnedObject(addr_ref)),
                            sender_arg,
                        ],
                        PARTY_RESTART_STREAM,
                        PartyStep::RestartStream,
                    ),
                    self.set_value(old_stream, 4, PARTY_OLD_STREAM, PartyStep::OldStream),
                ]
            }
        }
    }

    fn handle_result(&mut self, step: LifecycleStep, status: &BatchedTransactionStatus) {
        let LifecycleStep::Party(step) = step else {
            unreachable!("non-party step {step:?} routed to the party lifecycle");
        };
        let effects = match status {
            BatchedTransactionStatus::Success { effects } => effects,
            // A rejected tx did not change the object.
            BatchedTransactionStatus::PermanentFailure { .. } => return,
            BatchedTransactionStatus::RetriableFailure { .. }
            | BatchedTransactionStatus::UnknownRejection => {
                // These txs may have changed the object's ownership or owned version.
                if matches!(
                    step,
                    PartyStep::EndStream
                        | PartyStep::HandOff
                        | PartyStep::OwnedMutate
                        | PartyStep::RestartStream
                        | PartyStep::Wrap
                        | PartyStep::Unwrap
                        | PartyStep::HolderRead
                ) {
                    debug!(?step, "party tx outcome unknown, resetting lifecycle");
                    self.phase = PartyPhase::NeedCreate;
                }
                return;
            }
        };

        let next = match (step, &self.phase) {
            // Results of these txs never determine the phase.
            (PartyStep::OldStream | PartyStep::RacingMutate, _) => {
                if !effects.is_ok() {
                    assert_reachable!("party tx with an ended stream input executed");
                }
                return;
            }
            (PartyStep::Create, PartyPhase::NeedCreate) => {
                if !effects.is_ok() {
                    return;
                }
                effects.created().into_iter().find_map(|(obj_ref, owner)| {
                    Some(PartyPhase::Live {
                        id: obj_ref.0,
                        start_version: owner.start_version()?,
                        mutations: 0,
                    })
                })
            }
            (
                PartyStep::Mutate,
                PartyPhase::Live {
                    id,
                    start_version,
                    mutations,
                },
            ) => Some(PartyPhase::Live {
                id: *id,
                start_version: *start_version,
                mutations: mutations + 1,
            }),
            (
                PartyStep::Retransfer,
                PartyPhase::Live {
                    id,
                    start_version,
                    mutations,
                },
            ) => {
                if effects.is_ok() {
                    let owner = find_object(effects, *id).map(|(_, owner)| owner);
                    assert_eq!(
                        owner.and_then(|owner| owner.start_version()),
                        Some(*start_version),
                        "transfer to the same party owner must keep the stream"
                    );
                    assert_reachable!("party object transferred to its own party owner");
                }
                Some(PartyPhase::Live {
                    id: *id,
                    start_version: *start_version,
                    mutations: mutations + 1,
                })
            }
            (
                PartyStep::HandOff,
                PartyPhase::Live {
                    id, start_version, ..
                },
            ) => match find_object(effects, *id) {
                Some((
                    _,
                    Owner::ConsensusAddressOwner {
                        start_version: new_start,
                        ..
                    },
                )) if new_start != *start_version => {
                    assert_reachable!("party object transferred to a new party owner");
                    Some(PartyPhase::HandedOff {
                        id: *id,
                        ended_start_version: *start_version,
                    })
                }
                // The tx aborted or was cancelled, so the stream is still live.
                _ => return,
            },
            (
                PartyStep::Wrap,
                PartyPhase::Live {
                    id, start_version, ..
                },
            ) => {
                if !effects.wrapped().iter().any(|obj_ref| obj_ref.0 == *id) {
                    // The tx was cancelled, so the stream is still live.
                    return;
                }
                assert_reachable!("party object wrapped");
                let holder_ref = effects
                    .created()
                    .into_iter()
                    .find_map(|(obj_ref, owner)| {
                        matches!(owner, Owner::AddressOwner(_)).then_some(obj_ref)
                    })
                    .expect("the wrap tx creates the holder");
                Some(PartyPhase::Wrapped {
                    id: *id,
                    ended_start_version: *start_version,
                    holder_ref,
                })
            }
            (
                PartyStep::HolderRead,
                PartyPhase::Wrapped {
                    id,
                    ended_start_version,
                    holder_ref,
                },
            ) => {
                // The read bumped the holder's version, and the unwrap that raced it was
                // rejected.
                let Some((holder_ref, _)) = find_object(effects, holder_ref.0) else {
                    return;
                };
                Some(PartyPhase::Wrapped {
                    id: *id,
                    ended_start_version: *ended_start_version,
                    holder_ref,
                })
            }
            (
                PartyStep::Unwrap,
                PartyPhase::Wrapped {
                    id,
                    ended_start_version,
                    ..
                },
            ) => match find_object(effects, *id) {
                Some((addr_ref, Owner::AddressOwner(_))) => {
                    assert_reachable!("party object unwrapped to an address");
                    Some(PartyPhase::StreamEnded {
                        id: *id,
                        ended_start_version: *ended_start_version,
                        addr_ref,
                    })
                }
                Some((_, owner)) => owner.start_version().map(|start_version| {
                    assert_reachable!("party object unwrapped onto a new stream");
                    PartyPhase::Live {
                        id: *id,
                        start_version,
                        mutations: 0,
                    }
                }),
                // The tx was cancelled, so the object is still wrapped.
                None => return,
            },
            // The unwrap executed first and consumed the holder.
            (PartyStep::HolderRead, _) => return,
            (
                PartyStep::EndStream,
                PartyPhase::Live {
                    id, start_version, ..
                },
            ) => {
                match find_object(effects, *id) {
                    Some((addr_ref, Owner::AddressOwner(_))) => Some(PartyPhase::StreamEnded {
                        id: *id,
                        ended_start_version: *start_version,
                        addr_ref,
                    }),
                    // The tx aborted or was cancelled, so the stream is still live.
                    _ => return,
                }
            }
            (
                PartyStep::OwnedMutate | PartyStep::RestartStream,
                PartyPhase::StreamEnded {
                    id,
                    ended_start_version,
                    ..
                }
                | PartyPhase::Owned {
                    id,
                    ended_start_version,
                    ..
                },
            ) => {
                let rounds = match self.phase {
                    PartyPhase::Owned { rounds, .. } => rounds + 1,
                    _ => 1,
                };
                if effects.is_ok() {
                    if matches!(step, PartyStep::OwnedMutate) {
                        assert_reachable!("party object mutated as owned after its stream ended");
                    } else {
                        assert_reachable!("party stream restarted on an id with an ended stream");
                    }
                }
                match find_object(effects, *id) {
                    Some((addr_ref, Owner::AddressOwner(_))) => Some(PartyPhase::Owned {
                        id: *id,
                        ended_start_version: *ended_start_version,
                        addr_ref,
                        rounds,
                    }),
                    Some((_, owner)) => {
                        owner.start_version().map(|start_version| PartyPhase::Live {
                            id: *id,
                            start_version,
                            mutations: 0,
                        })
                    }
                    None => None,
                }
            }
            // An earlier tx in the same batch already reset the phase.
            (_, PartyPhase::NeedCreate) => return,
            (step, phase) => unreachable!("party tx {step:?} built in phase {phase:?}"),
        };

        self.phase = next.unwrap_or_else(|| {
            debug!(
                ?step,
                "party object missing from effects, resetting lifecycle"
            );
            PartyPhase::NeedCreate
        });
    }
}
