// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

// 1. Parent ID does not exist
// 2. Parent ID exists, but the field does not

//# programmable --sender A --inputs @A
//> 0: sui::bag::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getDynamicFieldObject",
  "params": [
    "0x0000000000000000000000000000000000000000000000000000000000000000",
    { "type": "u64", "value": "42" }
  ]
}

//# run-jsonrpc
{
  "method": "suix_getDynamicFieldObject",
  "params": [
    "@{obj_1_0}",
    { "type": "u64", "value": "42" }
  ]
}
