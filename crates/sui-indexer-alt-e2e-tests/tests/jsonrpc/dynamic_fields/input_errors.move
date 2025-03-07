// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

// 1. Type whose package doesn't exist
// 2. Type whose package is actually a move object
// 3. Type whose module doesn't exist
// 4. Type which doesn't exist in its module
// 5. Type parameter arity mismatch
// 6. Name that isn't valid JSON
// 7. Name that doesn't serialize correctly

//# publish
module P::M {
  public struct Key<phantom T> has copy, drop, store {
    x: u64
  }
}

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getDynamicFieldObject",
  "params": [
    "0x0000000000000000000000000000000000000000000000000000000000000000",
    { "type": "0x0000000000000000000000000000000000000000000000000000000000000000::M::Key<u64>", "value": "42" }
  ]
}

//# run-jsonrpc
{
  "method": "suix_getDynamicFieldObject",
  "params": [
    "0x0000000000000000000000000000000000000000000000000000000000000000",
    { "type": "@{obj_0_0}::M::Key<u64>", "value": "42" }
  ]
}

//# run-jsonrpc
{
  "method": "suix_getDynamicFieldObject",
  "params": [
    "0x0000000000000000000000000000000000000000000000000000000000000000",
    { "type": "@{P}::N::Key<u64>", "value": "42" }
  ]
}

//# run-jsonrpc
{
  "method": "suix_getDynamicFieldObject",
  "params": [
    "0x0000000000000000000000000000000000000000000000000000000000000000",
    { "type": "@{P}::M::DoesntExist<u64>", "value": "42" }
  ]
}

//# run-jsonrpc
{
  "method": "suix_getDynamicFieldObject",
  "params": [
    "0x0000000000000000000000000000000000000000000000000000000000000000",
    { "type": "@{P}::M::Key<u64, u32>", "value": "42" }
  ]
}

//# run-jsonrpc
{
  "method": "suix_getDynamicFieldObject",
  "params": [
    "0x0000000000000000000000000000000000000000000000000000000000000000",
    { "type": "@{P}::M::Key<u64>", "value": null }
  ]
}

//# run-jsonrpc
{
  "method": "suix_getDynamicFieldObject",
  "params": [
    "0x0000000000000000000000000000000000000000000000000000000000000000",
    { "type": "@{P}::M::Key<u64>", "value": "hello, world" }
  ]
}
