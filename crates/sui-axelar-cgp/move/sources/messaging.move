// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module axelar::messaging {
    use std::string;
    use std::string::String;

    #[test_only]
    use std::vector;

    /// Message struct which can consumed only by a `Channel` object.
    /// Does not require additional generic field to operate as linking
    /// by `id_bytes` is more than enough.
    ///
    /// Consider naming this `axelar::messaging::CallApproval`.
    struct Message {
        /// ID of the message, guaranteed to be unique by Axelar.
        msg_id: vector<u8>,
        /// The target Channel's UID.
        target_id: address,
        /// Name of the chain where this message came from.
        source_chain: String,
        /// Address of the source chain (vector used for compatibility).
        /// UTF8 / ASCII encoded string (for 0x0... eth address gonna be 42 bytes with 0x)
        source_address: String,
        /// Hash of the full payload (including source_* fields).
        payload_hash: vector<u8>,
        /// The rest of the payload to be used by the application.
        payload: vector<u8>,
    }

    struct StoredMessage has store {
        /// The target Channel's UID.
        target_id: address,
        /// Name of the chain where this message came from.
        source_chain: String,
        /// Address of the source chain (vector used for compatibility).
        /// UTF8 / ASCII encoded string (for 0x0... eth address gonna be 42 bytes with 0x)
        source_address: String,
        /// Hash of the full payload (including source_* fields).
        payload_hash: vector<u8>,
        /// The rest of the payload to be used by the application.
        payload: vector<u8>,
    }

    public fun create(
        msg_id: vector<u8>,
        source_chain: vector<u8>,
        source_address: vector<u8>,
        target_id: address,
        payload_hash: vector<u8>,
        payload: vector<u8>): Message {
        Message {
            msg_id,
            source_chain: string::utf8(source_chain),
            source_address: string::utf8(source_address),
            target_id,
            payload_hash,
            payload,
        }
    }

    public fun from_stored_message(msg_id: vector<u8>, msg: StoredMessage): Message {
        let StoredMessage {
            target_id,
            source_chain,
            source_address,
            payload_hash,
            payload,
        } = msg;

        Message {
            msg_id,
            source_chain,
            source_address,
            target_id,
            payload_hash,
            payload,
        }
    }

    public fun to_stored_message(msg: Message): (vector<u8>, StoredMessage) {
        let Message {
            msg_id,
            target_id,
            source_chain,
            source_address,
            payload_hash,
            payload,
        } = msg;

        (msg_id, StoredMessage {
            target_id,
            source_chain,
            source_address,
            payload_hash,
            payload,
        })
    }

    public fun consume_message(msg: Message): (String, String, vector<u8>, vector<u8>) {
        let Message {
            msg_id : _,
            target_id : _,
            source_chain,
            source_address,
            payload_hash,
            payload,
        } = msg;
        (source_chain, source_address, payload_hash, payload)
    }

    public fun target_id(msg: &Message): address {
        msg.target_id
    }

    #[test_only]
    /// Handy method for burning `vector<Message>` returned by the `execute` function.
    public fun delete(msgs: vector<Message>) {
        while (vector::length(&msgs) > 0) {
            let Message {
                msg_id: _,
                target_id: _,
                source_chain: _,
                source_address: _,
                payload_hash: _,
                payload: _
            } = vector::pop_back(&mut msgs);
        };
        vector::destroy_empty(msgs);
    }
}
