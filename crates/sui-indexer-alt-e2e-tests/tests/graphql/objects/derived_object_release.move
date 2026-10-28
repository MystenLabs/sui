// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A derived object is released and re-created, across checkpoints and within one checkpoint (with
// a different type). Its history spans every incarnation, and reads at a checkpoint where it is
// released return nothing.

//# init --accounts A --addresses test=0x0 --simulator

//# publish --sender A
module test::derived {
  use sui::derived_object;

  public struct Parent has key, store {
    id: UID,
  }

  public struct Child has key, store {
    id: UID,
    value: u64,
  }

  public struct Other has key, store {
    id: UID,
  }

  public fun new(ctx: &mut TxContext): Parent {
    Parent { id: object::new(ctx) }
  }

  public fun claim(parent: &mut Parent, value: u64): Child {
    Child { id: derived_object::claim(&mut parent.id, 0u64), value }
  }

  public fun claim_other(parent: &mut Parent): Other {
    Other { id: derived_object::claim(&mut parent.id, 0u64) }
  }

  public fun release(parent: &mut Parent, child: Child) {
    let Child { id, value: _ } = child;
    derived_object::release(&mut parent.id, id)
  }
}

//# programmable --sender A --inputs @A
//> 0: test::derived::new();
//> 1: TransferObjects([Result(0)], Input(0));

//# programmable --sender A --inputs object(2,0) 1u64 @A
//> 0: test::derived::claim(Input(0), Input(1));
//> 1: TransferObjects([Result(0)], Input(2));

//# create-checkpoint

//# run-graphql
{ # A owns the object after checkpoint 1.
  address(address: "@{A}") {
    objects(filter: { type: "@{test}::derived::Child" }) {
      nodes { address version }
    }
  }
}

//# programmable --sender A --inputs object(2,0) object(3,1)
//> 0: test::derived::release(Input(0), Input(1));

//# create-checkpoint

//# run-graphql
{ # The released object is gone from A's objects after checkpoint 2.
  address(address: "@{A}") {
    objects(filter: { type: "@{test}::derived::Child" }) {
      nodes { address version }
    }
  }
}

//# programmable --sender A --inputs object(2,0) 2u64 @A
//> 0: test::derived::claim(Input(0), Input(1));
//> 1: TransferObjects([Result(0)], Input(2));

//# create-checkpoint

//# run-graphql
{ # A owns the re-created object after checkpoint 3.
  address(address: "@{A}") {
    objects(filter: { type: "@{test}::derived::Child" }) {
      nodes { address version }
    }
  }
}

// Released and re-created as a different type in separate transactions of one checkpoint.
//# programmable --sender A --inputs object(2,0) object(3,1)
//> 0: test::derived::release(Input(0), Input(1));

//# programmable --sender A --inputs object(2,0) @A
//> 0: test::derived::claim_other(Input(0));
//> 1: TransferObjects([Result(0)], Input(1));

//# create-checkpoint

//# run-graphql
{
  objectVersions(address: "@{obj_3_1}") {
    nodes { ...Obj }
  }
}

fragment Obj on Object {
  version
  asMoveObject { contents { type { repr } json } }
}

//# run-graphql
{
  cp1: object(address: "@{obj_3_1}", atCheckpoint: 1) { ...Obj }
  cp2: object(address: "@{obj_3_1}", atCheckpoint: 2) { ...Obj }
  cp3: object(address: "@{obj_3_1}", atCheckpoint: 3) { ...Obj }
  cp4: object(address: "@{obj_3_1}", atCheckpoint: 4) { ...Obj }
  latest: object(address: "@{obj_3_1}") { ...Obj }
  # In checkpoint 4, the release at version 7 falls between the Child at 6 and the Other at 8.
  root6: object(address: "@{obj_3_1}", rootVersion: 6) { ...Obj }
  root7: object(address: "@{obj_3_1}", rootVersion: 7) { ...Obj }
  root8: object(address: "@{obj_3_1}", rootVersion: 8) { ...Obj }
}

fragment Obj on Object {
  version
  asMoveObject { contents { type { repr } json } }
}

//# run-graphql
{
  address(address: "@{obj_2_0}") {
    cp1: derivedObject(name: { literal: "0u64" }, atCheckpoint: 1) { ...Obj }
    cp2: derivedObject(name: { literal: "0u64" }, atCheckpoint: 2) { ...Obj }
    cp3: derivedObject(name: { literal: "0u64" }, atCheckpoint: 3) { ...Obj }
    cp4: derivedObject(name: { literal: "0u64" }, atCheckpoint: 4) { ...Obj }
    root6: derivedObject(name: { literal: "0u64" }, rootVersion: 6) { ...Obj }
    root7: derivedObject(name: { literal: "0u64" }, rootVersion: 7) { ...Obj }
    root8: derivedObject(name: { literal: "0u64" }, rootVersion: 8) { ...Obj }
  }
}

fragment Obj on MoveObject {
  version
  contents { type { repr } json }
}

//# run-graphql
{
  claimed: address(address: "@{obj_3_1}") {
    asTransactionObject(transactionDigest: "@{digest_3}") { ...Change }
  }
  released: address(address: "@{obj_3_1}") {
    asTransactionObject(transactionDigest: "@{digest_6}") { ...Change }
  }
  reclaimed: address(address: "@{obj_3_1}") {
    asTransactionObject(transactionDigest: "@{digest_9}") { ...Change }
  }
  releasedAgain: address(address: "@{obj_3_1}") {
    asTransactionObject(transactionDigest: "@{digest_12}") { ...Change }
  }
  reclaimedAsOther: address(address: "@{obj_3_1}") {
    asTransactionObject(transactionDigest: "@{digest_13}") { ...Change }
  }
}

fragment Change on ObjectChange {
  idCreated
  idDeleted
  inputState { version }
  outputState { version }
}

//# run-graphql
{
  address(address: "@{A}") {
    children: objects(filter: { type: "@{test}::derived::Child" }) {
      nodes { address version }
    }
    others: objects(filter: { type: "@{test}::derived::Other" }) {
      nodes { address version }
    }
  }
}
