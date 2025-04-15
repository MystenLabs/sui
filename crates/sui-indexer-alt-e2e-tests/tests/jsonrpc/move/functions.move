// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

//  1. Public function from v1 of a package
//  2. An entry function from v1 of a package
//  3. Same public function from v2 of a package
//  4. Same entry function at v2, with a different signature
//  5. A new function introduced in v2 of a package, using a v2 type.
//  6. Attempting to get a function with invalid identifiers.
//  7. Attempting to get a function from an object that doesn't exist.
//  8. Attempting to get a function from a move object (not a package)
//  9. Attempting to get a function from a module that doesn't exist.
// 10. Attempting to get a function that doesn't exist in its module.

//# publish --upgradeable --sender A
module P::M {
  public struct O has key {
    id: UID,
  }

  public fun foo(_: u64, o: O, p: &mut O, q: vector<O>): (u32, &O, O, vector<O>) {
    (0, p, o, q)
  }

  entry fun bar(_: u16) {}
}

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_getNormalizedMoveFunction",
  "params": ["@{P}", "M", "foo"]
}

//# run-jsonrpc
{
  "method": "sui_getNormalizedMoveFunction",
  "params": ["@{P}", "M", "bar"]
}

//# upgrade --package P --sender A --upgrade-capability 1,1
module P::M {
  public struct O has key {
    id: UID,
  }

  public struct O2 has key {
    id: UID,
  }

  public fun foo(_: u64, o: O, p: &mut O, q: vector<O>): (u32, &O, O, vector<O>) {
    (0, p, o, q)
  }

  entry fun bar(_: u16, _: u8, _: &O2) {}

  public fun baz(a: bool): u128 {
    if (a) 1 else 0
  }
}

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_getNormalizedMoveFunction",
  "params": ["@{P}", "M", "foo"]
}

//# run-jsonrpc
{
  "method": "sui_getNormalizedMoveFunction",
  "params": ["@{P}", "M", "bar"]
}

//# run-jsonrpc
{
  "method": "sui_getNormalizedMoveFunction",
  "params": ["@{P}", "M", "baz"]
}

//# run-jsonrpc
{
  "method": "sui_getNormalizedMoveFunction",
  "params": ["@{P}", "not a module", "foo"]
}

//# run-jsonrpc
{
  "method": "sui_getNormalizedMoveFunction",
  "params": ["0x0", "M", "foo"]
}

//# run-jsonrpc
{
  "method": "sui_getNormalizedMoveFunction",
  "params": ["@{obj_0_0}", "M", "foo"]
}

//# run-jsonrpc
{
  "method": "sui_getNormalizedMoveFunction",
  "params": ["@{P}", "N", "foo"]
}

//# run-jsonrpc
{
  "method": "sui_getNormalizedMoveFunction",
  "params": ["@{P}", "M", "qux"]
}
