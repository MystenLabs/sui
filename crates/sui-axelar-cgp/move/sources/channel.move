// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module axelar::channel {
    use sui::bcs;
    use sui::object;
    use sui::object::UID;
    use sui::tx_context::TxContext;
    use sui::vec_set;
    use sui::vec_set::VecSet;

    use axelar::messaging::{Self, CallApproval};
    use axelar::validators::{Self, AxelarValidators};

    /// Generic target for the messaging system.
    ///
    /// This struct is required on the Sui side to be the destination for the
    /// messages sent from other chains. Even though it has a UID field, it does
    /// not have a `key` ability to force wrapping.
    ///
    /// Notes:
    ///
    /// - Does not have key to prevent 99% of the mistakes related to access management.
    /// Also prevents arbitrary Message destruction if the object is shared. Lastly,
    /// when shared, `Channel` cannot be destroyed, and its contents will remain locked
    /// forever.
    ///
    /// - Allows asset or capability-locking inside. Some applications might
    /// authorize admin actions through the bridge (eg by locking some `AdminCap`
    /// inside and getting a `&mut AdminCap` in the `consume_message`);
    ///
    /// - Can be destroyed freely as the `UID` is guaranteed to be unique across
    /// the system. Destroying a channel would mean the end of the Channel cycle
    /// and all further messages will have to target a new Channel if there is one.
    ///
    /// - Does not contain direct link to the state in Sui, as some functions
    /// might not take any specific data (eg allow users to create new objects).
    /// If specific object on Sui is targeted by this `Channel`, its reference
    /// should be implemented using the `data` field.
    ///
    /// - The funniest and extremely simple implementation would be a `Channel<ID>`
    /// since it actually contains the data required to point at the object in Sui.

    /// For when trying to consume the wrong object.
    const EWrongDestination: u64 = 0;
    /// For when message has already been processed and submitted twice.
    const EDuplicateMessage: u64 = 2;

    struct Channel<T: store> has store {
        /// Unique ID of the target object which allows message targeting
        /// by comparing against `id_bytes`.
        id: UID,
        /// Messages processed by this object for the current axelar epoch. To make system less
        /// centralized, and spread the storage + io costs across multiple
        /// destinations, we can track every `Channel`'s messages.
        processed_call_approvals: VecSet<vector<u8>>,
        /// epoch of the last processed approval.
        last_processed_approval_epoch: u64,
        /// Additional field to optionally use as metadata for the Channel
        /// object improving identification and uniqueness of data.
        /// Can store any struct that has `store` ability (including other
        /// objects - eg Capabilities).
        data: T
    }

    /// Emitted when a new message is sent from the SUI network.
    struct ContractCall has copy, drop {
        source: vector<u8>,
        destination: vector<u8>,
        destination_address: vector<u8>,
        payload: vector<u8>,
    }

    /// Create new `Channel<T>` object. Anyone can create their own `Channel` to target
    /// from the outside and there's no limitation to the data stored inside it.
    ///
    /// `copy` ability is required to disallow asset locking inside the `Channel`.
    public fun create_channel<T: store>(t: T, ctx: &mut TxContext): Channel<T> {
        Channel {
            id: object::new(ctx),
            processed_call_approvals: vec_set::empty(),
            last_processed_approval_epoch: 0,
            data: t
        }
    }

    /// Destroy a `Channel<T>` releasing the T. Not constrained and can be performed
    /// by any party as long as they own a Channel.
    public fun destroy_channel<T: store>(self: Channel<T>): T {
        let Channel { id, processed_call_approvals: _, last_processed_approval_epoch: _, data } = self;
        object::delete(id);
        data
    }

    /// Send a message to another chain. Supply the event data and the
    /// destination chain.
    ///
    /// Event data is collected from the Channel (eg ID of the source and
    /// source_chain is a constant).
    public fun call_contract<T: store>(
        t: &mut Channel<T>,
        destination: vector<u8>,
        destination_address: vector<u8>,
        payload: vector<u8>
    ) {
        sui::event::emit(ContractCall {
            source: bcs::to_bytes(&t.id),
            destination,
            destination_address,
            payload,
        })
    }

    /// By using &mut here we make sure that the object is not in the freeze
    /// state and the owner has full access to the target.
    ///
    /// Most common scenario would be to target a shared object, however this
    /// messaging system allows sending private messages which can be consumed
    /// by single-owner targets.
    ///
    /// For Capability-locking, a mutable reference to the `Channel.data` field is
    /// returned; plus the hot potato message object.
    public fun retrieve_call_approval<T: store>(
        axelar: &mut AxelarValidators,
        t: &mut Channel<T>,
        cmd_id: vector<u8>,
    ): (&mut T, CallApproval) {
        let message = validators::remove_call_approval(axelar, cmd_id);

        let current_epoch = validators::epoch(axelar);

        if (t.last_processed_approval_epoch != current_epoch) {
            t.processed_call_approvals = vec_set::empty();
            t.last_processed_approval_epoch = current_epoch;
        };

        assert!(!vec_set::contains(&t.processed_call_approvals, &cmd_id), EDuplicateMessage);
        assert!(messaging::target_id(&message) == object::uid_to_address(&t.id), EWrongDestination);

        vec_set::insert(&mut t.processed_call_approvals, cmd_id);
        (&mut t.data, message)
    }
}
