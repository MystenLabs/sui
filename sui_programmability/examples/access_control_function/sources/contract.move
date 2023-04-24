// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This example shows how to update an object field without the need of a Permission Capability.
module access_control_function::contract {

    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::ecdsa_k1;
    use sui::bcs;

    use std::vector;

    //constants

    /// the public key that the check is against
    const PUBKEY: vector<u8> = x"03413b82d1c71e134d053bfd8db63a7eab006fec5eb038451be1eb11a99dc2c0d4";

    /// errors
    const EAlreadyLeveled: u64 = 0;
    const ESignatureNotVerified: u64 = 1;

    /// simple NFT that has level
    struct Editable has key, store {
        id: UID,
        field: u64,
    }

    public entry fun mint(ctx: &mut TxContext) {

        let id = object::new(ctx);
        let edit = Editable{id, field: 0};

        transfer::transfer(edit, tx_context::sender(ctx));

    }

    /// @param: signature The signature of the msg with our private secp256k1 key
    /// @param: hash 0 for Kekkak and 1 for SHA256, it should be always 1
    public entry fun edit_field(edit: &mut Editable, field: u64, signature: vector<u8>, hash: u8) {

        assert!(edit.field < field, EAlreadyLeveled);

        // construct the msg
        let msg: vector<u8> = bcs::to_bytes<UID>(&edit.id);
        vector::append(&mut msg, bcs::to_bytes<u64>(&field));

        // check public key
        let ver = ecdsa_k1::secp256k1_verify(&signature, &PUBKEY, &msg, hash);
        assert!(ver, ESignatureNotVerified);

        edit.field = field;
    }

    // getters
    // accessors
    public entry fun field(self: &Editable): u64 {
        self.field
    }
}

// Tests.

#[test_only]
module access_control_function::tests {
     use sui::test_scenario as ts;

     use access_control_function::contract::{Self, Editable};

     #[test]
     fun test_sig() {
        let user = @0x3301;
        let test = ts::begin(user);
        contract::mint(ts::ctx(&mut test));

        ts::next_tx(&mut test, user);

        let signature: vector<u8> = vector[
            108,159,156,171,131,175,188,25,54,52,174,141,20,236,26,172,90,54,164,
            246,52,89,176,192,110,49,131,82,76,139,45,10,46,204,155,126,96,198,228,
            61,123,208,28,169,139,66,34,5,1,154,53,183,216,52,20,7,37,255,77,246,
            174,221,120,8
        ];

        let hash: u8 = 1;

        let edit = ts::take_from_sender<Editable>(&test);

        contract::edit_field(&mut edit, 10, signature, hash);

        // ts::next_transaction(&test, user);
        assert!(10u64 == contract::field(&edit), 0);
        ts::return_to_sender<Editable>(&test, edit);

        ts::end(test);
     }
}
