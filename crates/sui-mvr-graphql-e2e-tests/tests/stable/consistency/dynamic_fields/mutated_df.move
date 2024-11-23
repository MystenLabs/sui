// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// version | status
// --------|--------
// 2       | created
// 3       | added df1, df2, df3
// 4       | mutated parent
// 5       | mutated df1
// 6       | mutated parent again

//# init --protocol-version 51 --addresses Test=0x0 --accounts A --simulator

//# publish
module Test::M1 {
    use sui::dynamic_field as field;
    use std::string::{String, utf8};

    public struct Parent has key, store {
        id: UID,
        count: u64
    }

    public entry fun parent(recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Parent { id: object::new(ctx), count: 0 },
            recipient
        )
    }

    public entry fun mutate_parent(parent: &mut Parent) {
        parent.count = parent.count + 42;
    }

    public entry fun add_df(obj: &mut Parent) {
        let id = &mut obj.id;
        field::add<String, String>(id, utf8(b"df1"), utf8(b"df1"));
        field::add<String, String>(id, utf8(b"df2"), utf8(b"df2"));
        field::add<String, String>(id, utf8(b"df3"), utf8(b"df3"));
    }

    public entry fun mutate_df1(parent: &mut Parent) {
        *field::borrow_mut(&mut parent.id, utf8(b"df1")) = utf8(b"df1_mutated");
    }
}

//# run Test::M1::parent --sender A --args @A

//# view-object 2,0

//# run Test::M1::add_df --sender A --args object(2,0)

//# view-object 2,0

//# run Test::M1::mutate_parent --sender A --args object(2,0)

//# view-object 2,0

//# create-checkpoint

//# run-graphql
fragment DynamicFieldSelect on DynamicField {
  name {
    bcs
  }
  value {
    ... on MoveObject {
      contents {
        json
      }
    }
    ... on MoveValue {
      json
    }
  }
}

fragment DynamicFieldsSelect on DynamicFieldConnection {
  edges {
    cursor
    node {
      ...DynamicFieldSelect
    }
  }
}

{
  latest: object(address: "@{obj_2_0}") {
    version
    dynamicFields {
      ...DynamicFieldsSelect
    }
    dynamicField(name: {type: "0x0000000000000000000000000000000000000000000000000000000000000001::string::String", bcs: "A2RmMQ=="}) {
        ...DynamicFieldSelect
    }
  }
  df_added: object(address: "@{obj_2_0}", version: 3) {
    version
    dynamicField(name: {type: "0x0000000000000000000000000000000000000000000000000000000000000001::string::String", bcs: "A2RmMQ=="}) {
        ...DynamicFieldSelect
    }
  }
  before_df_added: object(address: "@{obj_2_0}", version: 2) {
    version
    dynamicField(name: {type: "0x0000000000000000000000000000000000000000000000000000000000000001::string::String", bcs: "A2RmMQ=="}) {
        ...DynamicFieldSelect
    }
  }
}

//# view-object 2,0

//# run Test::M1::mutate_df1 --sender A --args object(2,0)

//# view-object 2,0

//# run Test::M1::mutate_parent --sender A --args object(2,0)

//# view-object 2,0

//# create-checkpoint

//# run-graphql
fragment DynamicFieldSelect on DynamicField {
  name {
    bcs
  }
  value {
    ... on MoveObject {
      contents {
        json
      }
    }
    ... on MoveValue {
      json
    }
  }
}

fragment DynamicFieldsSelect on DynamicFieldConnection {
  edges {
    cursor
    node {
      ...DynamicFieldSelect
    }
  }
}

{
  latest: object(address: "@{obj_2_0}") {
    version
    dynamicFields {
      ...DynamicFieldsSelect
    }
    dynamicField(name: {type: "0x0000000000000000000000000000000000000000000000000000000000000001::string::String", bcs: "A2RmMQ=="}) {
        ...DynamicFieldSelect
    }
  }
  df1_mutated: object(address: "@{obj_2_0}", version: 5) {
    version
    dynamicField(name: {type: "0x0000000000000000000000000000000000000000000000000000000000000001::string::String", bcs: "A2RmMQ=="}) {
        ...DynamicFieldSelect
    }
  }
  before_df1_mutated: object(address: "@{obj_2_0}", version: 4) {
    version
    dynamicField(name: {type: "0x0000000000000000000000000000000000000000000000000000000000000001::string::String", bcs: "A2RmMQ=="}) {
        ...DynamicFieldSelect
    }
  }
}
