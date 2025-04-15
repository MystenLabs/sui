// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

// 1. Multi-get objects (with found object, with non-existent object, with deleted object)
// 2. Multi-get objects with options

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 43 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 44
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::transfer::public_share_object<sui::coin::Coin<sui::sui::SUI>>(Result(0))

//# programmable --sender A --inputs 45
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::transfer::public_freeze_object<sui::coin::Coin<sui::sui::SUI>>(Result(0))

//# programmable --sender A --inputs @A
//> 0: sui::table::new<u64, u64>();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs object(5,0) 46 47
//> 0: sui::table::add<u64, u64>(Input(0), Input(1), Input(2))

//# programmable --sender A --inputs object(5,0) 46
//> 0: sui::table::remove<u64, u64>(Input(0), Input(1))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_tryMultiGetPastObjects",
  "params": [
    [
      { "objectId": "@{obj_1_0}", "version": "2" },
      { "objectId": "@{obj_2_0}", "version": "3" },
      { "objectId": "@{obj_3_0}", "version": "4" },
      { "objectId": "@{obj_4_0}", "version": "5" },
      { "objectId": "@{obj_5_0}", "version": "6" },
      { "objectId": "@{obj_6_0}", "version": "7" },
      { "objectId": "@{obj_6_0}", "version": "8" },
      { "objectId": "@{obj_6_0}", "version": "9" }
    ]
  ]
}

//# run-jsonrpc
{
  "method": "sui_tryMultiGetPastObjects",
  "params": [
    [
      { "objectId": "@{obj_1_0}", "version": "2" },
      { "objectId": "@{obj_2_0}", "version": "3" },
      { "objectId": "@{obj_3_0}", "version": "4" },
      { "objectId": "@{obj_4_0}", "version": "5" },
      { "objectId": "@{obj_5_0}", "version": "6" },
      { "objectId": "@{obj_6_0}", "version": "7" },
      { "objectId": "@{obj_6_0}", "version": "8" },
      { "objectId": "@{obj_6_0}", "version": "9" }
    ],
    {
      "showType": true,
      "showOwner": true,
      "showPreviousTransaction": true,
      "showStorageRebate": true
    }
  ]
}
