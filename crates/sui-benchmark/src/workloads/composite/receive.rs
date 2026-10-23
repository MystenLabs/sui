// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_common::assert_reachable;
use mysten_common::random::get_rng;
use rand::Rng;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::object::Owner;
use sui_types::transaction::{CallArg, ObjectArg};
use tracing::debug;

use super::lifecycle::{
    Lifecycle, LifecycleCall, LifecycleStep, StepContext, find_object, pure, shared_mut,
};
use super::operations::{
    RECEIVE_CHILD, RECEIVE_CREATE_CHILD, RECEIVE_CREATE_PARENT, RECEIVE_SEND_CHILD,
    RECEIVE_TOUCH_PARENT,
};
use crate::ExecutionEffects;
use crate::workloads::payload::BatchedTransactionStatus;

#[derive(Clone, Copy, Debug)]
pub(super) enum ReceiveStep {
    /// Create a parent object.
    CreateParent,
    /// Create a child owned by the parent.
    CreateChild,
    /// Send the sender's child to the parent.
    SendChild,
    /// Receive the child from the parent and send it to the sender.
    Receive,
    /// Mutate the parent, submitted alongside a receive.
    TouchParent,
}

#[derive(Clone, Copy, Debug)]
enum ParentKind {
    Owned,
    Shared,
    Party,
}

#[derive(Clone, Copy, Debug)]
enum Parent {
    Owned(ObjectRef),
    /// A shared or party parent, identified by its id and the start of its consensus stream.
    Consensus {
        kind: ParentKind,
        id: ObjectID,
        start_version: SequenceNumber,
    },
}

impl Parent {
    fn id(&self) -> ObjectID {
        match self {
            Parent::Owned(obj_ref) => obj_ref.0,
            Parent::Consensus { id, .. } => *id,
        }
    }

    fn kind(&self) -> ParentKind {
        match self {
            Parent::Owned(_) => ParentKind::Owned,
            Parent::Consensus { kind, .. } => *kind,
        }
    }

    fn arg(&self) -> CallArg {
        match self {
            Parent::Owned(obj_ref) => CallArg::Object(ObjectArg::ImmOrOwnedObject(*obj_ref)),
            Parent::Consensus {
                id, start_version, ..
            } => shared_mut(*id, *start_version),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Child {
    None,
    AtParent(ObjectRef),
    AtSender(ObjectRef),
}

/// A payload's transfer-to-object lifecycle. It creates a parent that is owned, shared or
/// party-owned, then repeatedly sends a child to the parent's address and receives it back.
/// Some receives are submitted twice, or together with a mutation of the parent.
#[derive(Debug, Clone)]
enum ReceivePhase {
    NeedParent,
    Active {
        parent: Parent,
        child: Child,
        receives: u32,
    },
}

#[derive(Debug)]
pub(super) struct ReceiveLifecycle {
    sender: SuiAddress,
    start_probability: f64,
    phase: ReceivePhase,
}

impl ReceiveLifecycle {
    /// The parent is abandoned after this many receives so that each parent kind is used.
    const MAX_RECEIVES: u32 = 3;

    pub fn new(sender: SuiAddress, start_probability: f64) -> Self {
        Self {
            sender,
            start_probability,
            phase: ReceivePhase::NeedParent,
        }
    }

    fn call(
        &self,
        function: &'static str,
        args: Vec<CallArg>,
        op: &'static str,
        step: ReceiveStep,
    ) -> LifecycleCall {
        LifecycleCall::move_call("receiver", function, args, op, LifecycleStep::Receive(step))
    }

    fn parent_from_effects(effects: &ExecutionEffects, sender: SuiAddress) -> Option<Parent> {
        // The create tx is bare, so the parent is the only object it creates.
        let (obj_ref, owner) = effects.created().into_iter().next()?;
        match owner {
            Owner::AddressOwner(addr) if addr == sender => Some(Parent::Owned(obj_ref)),
            Owner::Shared {
                initial_shared_version,
            } => Some(Parent::Consensus {
                kind: ParentKind::Shared,
                id: obj_ref.0,
                start_version: initial_shared_version,
            }),
            Owner::ConsensusAddressOwner { start_version, .. } => Some(Parent::Consensus {
                kind: ParentKind::Party,
                id: obj_ref.0,
                start_version,
            }),
            _ => None,
        }
    }

    /// Updates the tracked parent and child from the objects in `effects`.
    fn update_from_effects(&mut self, effects: &ExecutionEffects, sender: SuiAddress) {
        let ReceivePhase::Active { parent, child, .. } = &mut self.phase else {
            return;
        };
        let parent_addr: SuiAddress = parent.id().into();
        if let Parent::Owned(obj_ref) = parent
            && let Some((new_ref, _)) = find_object(effects, obj_ref.0)
        {
            *obj_ref = new_ref;
        }
        let child_id = match child {
            Child::None => effects
                .created()
                .into_iter()
                .find(|(_, owner)| *owner == Owner::AddressOwner(parent_addr))
                .map(|(obj_ref, _)| obj_ref.0),
            Child::AtParent(obj_ref) | Child::AtSender(obj_ref) => Some(obj_ref.0),
        };
        let Some((child_ref, owner)) = child_id.and_then(|id| find_object(effects, id)) else {
            return;
        };
        match owner {
            Owner::AddressOwner(addr) if addr == parent_addr => {
                *child = Child::AtParent(child_ref);
            }
            Owner::AddressOwner(addr) if addr == sender => {
                *child = Child::AtSender(child_ref);
            }
            owner => unreachable!("child has unexpected owner {owner:?}"),
        }
    }
}

impl Lifecycle for ReceiveLifecycle {
    fn max_step_calls(&self) -> usize {
        2
    }

    fn in_progress(&self) -> bool {
        !matches!(self.phase, ReceivePhase::NeedParent)
    }

    fn start_probability(&self) -> f64 {
        self.start_probability
    }

    fn next_step(&mut self, ctx: &StepContext) -> Vec<LifecycleCall> {
        let mut rng = get_rng();
        if let ReceivePhase::Active {
            receives,
            child: Child::AtSender(_),
            ..
        } = self.phase
            && receives >= Self::MAX_RECEIVES
        {
            self.phase = ReceivePhase::NeedParent;
        }

        let sender_arg = pure(&ctx.sender);
        match self.phase {
            ReceivePhase::NeedParent => {
                let (function, args) = match rng.gen_range(0..3) {
                    0 => ("create_owned_parent", vec![sender_arg]),
                    1 => ("create_shared_parent", vec![]),
                    _ => ("create_party_parent", vec![sender_arg]),
                };
                vec![
                    self.call(
                        function,
                        args,
                        RECEIVE_CREATE_PARENT,
                        ReceiveStep::CreateParent,
                    )
                    .bare(),
                ]
            }
            ReceivePhase::Active { parent, child, .. } => {
                let parent_addr: SuiAddress = parent.id().into();
                match child {
                    Child::None => vec![self.call(
                        "create_child",
                        vec![pure(&parent_addr)],
                        RECEIVE_CREATE_CHILD,
                        ReceiveStep::CreateChild,
                    )],
                    Child::AtSender(child_ref) => vec![self.call(
                        "send_child",
                        vec![
                            CallArg::Object(ObjectArg::ImmOrOwnedObject(child_ref)),
                            pure(&parent_addr),
                        ],
                        RECEIVE_SEND_CHILD,
                        ReceiveStep::SendChild,
                    )],
                    Child::AtParent(child_ref) => {
                        let receive = self.call(
                            "receive_child",
                            vec![
                                parent.arg(),
                                CallArg::Object(ObjectArg::Receiving(child_ref)),
                                sender_arg,
                            ],
                            RECEIVE_CHILD,
                            ReceiveStep::Receive,
                        );
                        let mut calls = vec![receive.clone()];
                        match rng.gen_range(0..3) {
                            // A second receive of the same child version.
                            0 => calls.push(receive),
                            1 => calls.push(self.call(
                                "touch",
                                vec![parent.arg()],
                                RECEIVE_TOUCH_PARENT,
                                ReceiveStep::TouchParent,
                            )),
                            _ => {}
                        }
                        calls
                    }
                }
            }
        }
    }

    fn handle_result(&mut self, step: LifecycleStep, status: &BatchedTransactionStatus) {
        let LifecycleStep::Receive(step) = step else {
            unreachable!("step {step:?} routed to the receive lifecycle");
        };
        let effects = match status {
            BatchedTransactionStatus::Success { effects } => effects,
            BatchedTransactionStatus::PermanentFailure { .. } => return,
            BatchedTransactionStatus::RetriableFailure { .. }
            | BatchedTransactionStatus::UnknownRejection => {
                // The tx may have changed the version of an owned parent or child.
                if !matches!(step, ReceiveStep::CreateParent) {
                    debug!(?step, "receive tx outcome unknown, resetting lifecycle");
                    self.phase = ReceivePhase::NeedParent;
                }
                return;
            }
        };
        let sender = self.sender;

        if let ReceiveStep::CreateParent = step {
            if let ReceivePhase::NeedParent = self.phase
                && let Some(parent) = Self::parent_from_effects(effects, sender)
            {
                self.phase = ReceivePhase::Active {
                    parent,
                    child: Child::None,
                    receives: 0,
                };
            }
            return;
        }

        if let (ReceiveStep::Receive, ReceivePhase::Active { parent, .. }) = (step, &self.phase) {
            if effects.is_ok() {
                match parent.kind() {
                    ParentKind::Owned => assert_reachable!("child received by an owned parent"),
                    ParentKind::Shared => assert_reachable!("child received by a shared parent"),
                    ParentKind::Party => assert_reachable!("child received by a party parent"),
                }
            } else if !effects.is_cancelled() {
                assert_reachable!("receive of a child failed in execution");
            }
        }

        self.update_from_effects(effects, sender);
        if let (ReceiveStep::Receive, ReceivePhase::Active { receives, .. }) =
            (step, &mut self.phase)
            && effects.is_ok()
        {
            *receives += 1;
        }
    }
}
