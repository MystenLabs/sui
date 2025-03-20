// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module struct_metadata::struct_metadata{
  use std::string::{Self, String};
  use sui::tx_context::{Self, TxContext};
  use sui::object::{Self, UID};
  use sui::transfer;
  use sui::dynamic_object_field as dof;
  
  struct Dummy  has key {
    id: UID,
    number: u64,
    description: String,
  }

  struct DummyDof has key, store {
    id: UID,
    description: String,
  }

  fun init(ctx: &mut TxContext){
    let id = object::new(ctx);
    let dummy = Dummy{id, number: 1, description: string::utf8(b"Hello")};
    let dof_uid = object::new(ctx);
    let dof_id = object::uid_to_inner(&dof_uid);
    let dummy_dof = DummyDof{id: dof_uid, description: string::utf8(b"dummy dof")};
    dof::add(&mut dummy.id, dof_id, dummy_dof);
    transfer::transfer(dummy, tx_context::sender(ctx))
  }
}