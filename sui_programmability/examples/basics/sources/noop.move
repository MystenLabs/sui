// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Noop application.
module basics::noop{
    use sui::tx_context::TxContext;
    use sui::object::{Self, UID};
    use std::string::{String};
    use sui::event::emit;
    use sui::tx_context;
    use sui::transfer;
 
    /*
        A struct that is used to emit an event with metadata.
    */
    struct MetaData has copy, drop{
        creator: address,
        metadata: vector<u8>
    }
    struct TestNFT has key, store {
        id: UID,
        owner: address,
        message: String
    }
    /*
        Empty heartbeat call.
    */
    public entry fun noop(){}
 
    /*
        Empty heartbeat call with metadata input.
    */
    public entry fun noop_w_metadata(_metadata: vector<u8>){
 
    }
 
    /*
        Heartbeat with metadata that emits an event.
    */
    public entry fun noop_w_metadata_event(metadata:vector<u8>, ctx: &mut TxContext){
        emit(MetaData {
            creator: tx_context::sender(ctx),
            metadata
        })
    }

    public entry fun noop_w_nft(
        message: String,
        ctx: &mut TxContext
    ) {
        let testNFT = TestNFT {
            id: object::new(ctx),
            owner: tx_context::sender(ctx),
            message
            
        };

        transfer::transfer(
            testNFT,
            tx_context::sender(ctx)
        )

    }
}