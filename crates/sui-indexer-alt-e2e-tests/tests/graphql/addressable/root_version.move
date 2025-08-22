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

//# programmable --sender A --inputs @A
//> 0: sui::bag::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::object_bag::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# programmable --sender A --inputs object(1,0) 1u64 object(2,0) 2u64 object(3,0) 3u64 object(4,0)
//> 0: sui::bag::add<u64, sui::bag::Bag>(Input(2), Input(3), Input(4));
//> 1: sui::bag::add<u64, sui::bag::Bag>(Input(2), Input(5), Input(6));
//> 2: sui::bag::add<u64, sui::bag::Bag>(Input(0), Input(1), Input(2));

//# programmable --sender A --inputs object(5,0) 400u64 500u64
//> 0: SplitCoins(Gas, [Input(1), Input(2)]);
//> 1: sui::object_bag::add<u64, sui::coin::Coin<sui::sui::SUI>>(Input(0), Input(1), NestedResult(0,0));
//> 2: sui::object_bag::add<u64, sui::coin::Coin<sui::sui::SUI>>(Input(0), Input(2), NestedResult(0,1));

//# create-checkpoint

//# run-graphql
{ # Only the outermost Bag is still accessible, the rest are wrapped.
  multiGetObjects(keys: [
    { address: "@{obj_1_0}" },
    { address: "@{obj_2_0}" },
    { address: "@{obj_3_0}" },
    { address: "@{obj_4_0}" },
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
        { address: "@{obj_4_0}" },
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

//# run-graphql --cursors bcs(2u64) bcs(3u64)
{ # The middle Bag is not accessible, so we need to use `address` to root
  # further dynamic field queries.
  address(address: "@{obj_2_0}", rootVersion: 7) {
    df2: dynamicField(name: { type: "u64", bcs: "@{cursor_0}" }) { ...DF }
    df3: dynamicField(name: { type: "u64", bcs: "@{cursor_1}" }) { ...DF }
    multiGetDynamicFields(keys: [
      { type: "u64", bcs: "@{cursor_0}" },
      { type: "u64", bcs: "@{cursor_1}" }
    ]) { ...DF }
  }
}

fragment DF on DynamicField {
  value { ... on MoveValue { json } }
}

//# run-graphql --cursors bcs(400u64) bcs(500u64)
{ # Accessing the elements of the ObjectBag via `object` and `address`.
  object(address: "@{obj_5_0}") {
    asMoveObject {
      df400: dynamicObjectField(name: { type: "u64", bcs: "@{cursor_0}" }) { ...DOF }
      df500: dynamicObjectField(name: { type: "u64", bcs: "@{cursor_1}" }) { ...DOF }
      multiGetDynamicObjectFields(keys: [
        { type: "u64", bcs: "@{cursor_0}" },
        { type: "u64", bcs: "@{cursor_1}" }
      ]) { ...DOF }
    }
  }

  address(address: "@{obj_5_0}", rootVersion: 8) {
    df400: dynamicObjectField(name: { type: "u64", bcs: "@{cursor_0}" }) { ...DOF }
    df500: dynamicObjectField(name: { type: "u64", bcs: "@{cursor_1}" }) { ...DOF }
    multiGetDynamicObjectFields(keys: [
      { type: "u64", bcs: "@{cursor_0}" },
      { type: "u64", bcs: "@{cursor_1}" }
    ]) { ...DOF }
  }
}

fragment DOF on DynamicField {
  value { ... on MoveObject { contents { json } } }
}

//# run-graphql
{ # Error - cannot query owned objects on an address with root version set
  address(address: "0x2", rootVersion: 1) {
    objects { nodes { address } }
  }
}
