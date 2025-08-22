// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# programmable --sender A --inputs @A
//> 0: sui::bag::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::bag::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::bag::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# programmable --sender A --inputs object(1,0) 1u64 object(2,0) 2u64 object(3,0)
//> 0: sui::bag::add<u64, sui::bag::Bag>(Input(2), Input(3), Input(4));
//> 1: sui::bag::add<u64, sui::bag::Bag>(Input(0), Input(1), Input(2));

//# create-checkpoint

//# run-graphql
{ # Only the outermost Bag is still accessible, the rest are wrapped.
  multiGetObjects(keys: [
    { address: "@{obj_1_0}" },
    { address: "@{obj_2_0}" },
    { address: "@{obj_3_0}" },
  ]) {
    version
    asMoveObject {
      contents { json }
    }
  }
}

//# run-graphql
{ # At the previous checkpoint, they were all accessible
  checkpoint(sequenceNumber: 1) {
    query {
      multiGetObjects(keys: [
        { address: "@{obj_1_0}" },
        { address: "@{obj_2_0}" },
        { address: "@{obj_3_0}" },
      ]) {
        version
        asMoveObject {
          contents { json }
        }
      }
    }
  }
}

//# run-graphql --cursors bcs(1u64)
{ # The outermost Bag can be accessed with `address` or `object`
  address(address: "@{obj_1_0}") {
    dynamicField(name: { type: "u64", bcs: "@{cursor_0}" }) {
      value { ... on MoveValue { json } }
    }
  }

  object(address: "@{obj_1_0}") {
    asMoveObject {
      dynamicField(name: { type: "u64", bcs: "@{cursor_0}" }) {
        value { ... on MoveValue { json } }
      }
    }
  }
}

//# run-graphql --cursors bcs(2u64)
{ # The middle Bag is not accessible, so we need to use `address` to root a
  # further dynamic field query.
  address(address: "@{obj_2_0}", rootVersion: 5) {
    dynamicField(name: { type: "u64", bcs: "@{cursor_0}" }) {
      value { ... on MoveValue { json } }
    }
  }
}

//# run-graphql
{ # Error - cannot query owned objects on an address with root version set
  address(address: "0x2", rootVersion: 1) {
    objects { nodes { address } }
  }
}
