// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P0=0x0 P1=0x0 --simulator

// Publishes and upgrades of user packages both show up as "Published" object
// changes.

//# publish --upgradeable --sender A
module P0::M {
  public fun f(): u64 { 42 }
}

//# upgrade --package P0 --upgrade-capability 1,1 --sender A
module P1::M {
  public fun f(): u64 { 42 }
}

module P1::N {
  public fun g(): u64 { 43 }
}

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_getTransactionBlock",
  "params": ["@{digest_1}", { "showObjectChanges": true }]
}

//# run-jsonrpc
{
  "method": "sui_getTransactionBlock",
  "params": ["@{digest_2}", { "showObjectChanges": true }]
}
