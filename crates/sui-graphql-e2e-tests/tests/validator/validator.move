// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --simulator --accounts A C --custom-validator-account --addresses P0=0x0 P1=0x0 P2=0x0 P3=0x0 P4=0x0 P5=0x0 P6=0x0 P7=0x0 P8=0x0 P9=0x0

//# advance-epoch

//# create-checkpoint

//# programmable --sender C --inputs 10000000000 @C
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1));

//# run 0x3::sui_system::request_add_stake --args object(0x5) object(3,0) @validator_0 --sender C

//# run 0x3::sui_system::request_set_commission_rate --sender validator_0 --args object(0x5) 0u64

//# run 0x3::sui_system::rotate_operation_cap --sender validator_0 --args object(0x5)

//# run 0x3::sui_system::request_set_gas_price --sender validator_0 --args object(0x5) object(6,0) 99999u64

//# create-checkpoint

//# advance-epoch

//# create-checkpoint

//# publish --sender A --gas-budget 9999999999
module P0::m {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::TxContext;
    use std::vector;

    struct Big has key {
        id: UID,
        weight: vector<u8>,
    }

    fun init(ctx: &mut TxContext) {
        transfer::share_object(Big {
          id: object::new(ctx),
          weight: weight(),
        });
    }

    fun weight(): vector<u8> {
        let i = 0;
        let v = vector[];
        while (i < 248 * 1024) {
            vector::push_back(&mut v, 42);
            i = i + 1;
        };
        v
    }
}

//# publish --sender A --gas-budget 9999999999
module P1::m {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::TxContext;
    use std::vector;

    struct Big has key {
        id: UID,
        weight: vector<u8>,
    }

    fun init(ctx: &mut TxContext) {
        transfer::share_object(Big {
          id: object::new(ctx),
          weight: weight(),
        });
    }

    fun weight(): vector<u8> {
        let i = 0;
        let v = vector[];
        while (i < 248 * 1024) {
            vector::push_back(&mut v, 42);
            i = i + 1;
        };
        v
    }
}


//# publish --sender A --gas-budget 9999999999
module P2::m {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::TxContext;
    use std::vector;

    struct Big has key {
        id: UID,
        weight: vector<u8>,
    }

    fun init(ctx: &mut TxContext) {
        transfer::share_object(Big {
          id: object::new(ctx),
          weight: weight(),
        });
    }

    fun weight(): vector<u8> {
        let i = 0;
        let v = vector[];
        while (i < 248 * 1024) {
            vector::push_back(&mut v, 42);
            i = i + 1;
        };
        v
    }
}

//# publish --sender A --gas-budget 9999999999
module P3::m {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::TxContext;
    use std::vector;

    struct Big has key {
        id: UID,
        weight: vector<u8>,
    }

    fun init(ctx: &mut TxContext) {
        transfer::share_object(Big {
          id: object::new(ctx),
          weight: weight(),
        });
    }

    fun weight(): vector<u8> {
        let i = 0;
        let v = vector[];
        while (i < 248 * 1024) {
            vector::push_back(&mut v, 42);
            i = i + 1;
        };
        v
    }
}

//# publish --sender A --gas-budget 9999999999
module P4::m {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::TxContext;
    use std::vector;

    struct Big has key {
        id: UID,
        weight: vector<u8>,
    }

    fun init(ctx: &mut TxContext) {
        transfer::share_object(Big {
          id: object::new(ctx),
          weight: weight(),
        });
    }

    fun weight(): vector<u8> {
        let i = 0;
        let v = vector[];
        while (i < 248 * 1024) {
            vector::push_back(&mut v, 42);
            i = i + 1;
        };
        v
    }
}

//# publish --sender A --gas-budget 9999999999
module P5::m {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::TxContext;
    use std::vector;

    struct Big has key {
        id: UID,
        weight: vector<u8>,
    }

    fun init(ctx: &mut TxContext) {
        transfer::share_object(Big {
          id: object::new(ctx),
          weight: weight(),
        });
    }

    fun weight(): vector<u8> {
        let i = 0;
        let v = vector[];
        while (i < 248 * 1024) {
            vector::push_back(&mut v, 42);
            i = i + 1;
        };
        v
    }
}

//# publish --sender A --gas-budget 9999999999
module P6::m {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::TxContext;
    use std::vector;

    struct Big has key {
        id: UID,
        weight: vector<u8>,
    }

    fun init(ctx: &mut TxContext) {
        transfer::share_object(Big {
          id: object::new(ctx),
          weight: weight(),
        });
    }

    fun weight(): vector<u8> {
        let i = 0;
        let v = vector[];
        while (i < 248 * 1024) {
            vector::push_back(&mut v, 42);
            i = i + 1;
        };
        v
    }
}

//# publish --sender A --gas-budget 9999999999
module P7::m {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::TxContext;
    use std::vector;

    struct Big has key {
        id: UID,
        weight: vector<u8>,
    }

    fun init(ctx: &mut TxContext) {
        transfer::share_object(Big {
          id: object::new(ctx),
          weight: weight(),
        });
    }

    fun weight(): vector<u8> {
        let i = 0;
        let v = vector[];
        while (i < 248 * 1024) {
            vector::push_back(&mut v, 42);
            i = i + 1;
        };
        v
    }
}

//# publish --sender A --gas-budget 9999999999
module P8::m {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::TxContext;
    use std::vector;

    struct Big has key {
        id: UID,
        weight: vector<u8>,
    }

    fun init(ctx: &mut TxContext) {
        transfer::share_object(Big {
          id: object::new(ctx),
          weight: weight(),
        });
    }

    fun weight(): vector<u8> {
        let i = 0;
        let v = vector[];
        while (i < 248 * 1024) {
            vector::push_back(&mut v, 42);
            i = i + 1;
        };
        v
    }
}

//# publish --sender A --gas-budget 9999999999
module P9::m {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::TxContext;
    use std::vector;

    struct Big has key {
        id: UID,
        weight: vector<u8>,
    }

    fun init(ctx: &mut TxContext) {
        transfer::share_object(Big {
          id: object::new(ctx),
          weight: weight(),
        });
    }

    fun weight(): vector<u8> {
        let i = 0;
        let v = vector[];
        while (i < 248 * 1024) {
            vector::push_back(&mut v, 42);
            i = i + 1;
        };
        v
    }
}

// TODO: Short term hack to get around indexer epoch issue
//# create-checkpoint

//# advance-epoch

// check the epoch metrics

//# run-graphql
{
  epoch{
    validatorSet {
      activeValidators {
        apy
        name
      }
    }
  }
}
