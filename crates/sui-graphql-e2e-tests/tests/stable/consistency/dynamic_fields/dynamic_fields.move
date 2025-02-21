// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// chkpt1: create parent and child @ version 2
// chkpt1: add dof to parent @ version 3
// chkpt2: PTB(mutate dof, add df1, 2, 3) - parent and dof @ version 4
// chkpt3: add df4, 5, 6 parent @ version 5, child @ version 4
// chkpt4: remove df1, df2, df3 parent @ version 6, child @ version 4

//# init --protocol-version 51 --addresses Test=0x0 --accounts A --simulator --objects-snapshot-min-checkpoint-lag 4

//# publish
module Test::M1 {
    use sui::dynamic_object_field as ofield;
    use sui::dynamic_field as field;
    use std::string::{String, utf8};

    public struct Parent has key, store {
        id: UID,
    }

    public struct Child has key, store {
        id: UID,
        count: u64,
    }

    public entry fun parent(recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Parent { id: object::new(ctx) },
            recipient
        )
    }

    public entry fun child(recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Child { id: object::new(ctx), count: 0 },
            recipient
        )
    }

    public fun add_child(parent: &mut Parent, child: Child, name: u64) {
        ofield::add(&mut parent.id, name, child);
    }

    public fun mutate_child(child: &mut Child) {
        child.count = child.count + 1;
    }

    public fun mutate_child_via_parent(parent: &mut Parent, name: u64) {
        mutate_child(ofield::borrow_mut(&mut parent.id, name))
    }

    public fun reclaim_child(parent: &mut Parent, name: u64): Child {
        ofield::remove(&mut parent.id, name)
    }

    public fun delete_child(parent: &mut Parent, name: u64) {
        let Child { id, count: _ } = reclaim_child(parent, name);
        object::delete(id);
    }

    public entry fun add_df(obj: &mut Parent) {
        let id = &mut obj.id;
        field::add<String, String>(id, utf8(b"df1"), utf8(b"df1"));
        field::add<String, String>(id, utf8(b"df2"), utf8(b"df2"));
        field::add<String, String>(id, utf8(b"df3"), utf8(b"df3"));
    }

    public entry fun remove_df(obj: &mut Parent) {
        let id = &mut obj.id;
        field::remove<String, String>(id, utf8(b"df1"));
        field::remove<String, String>(id, utf8(b"df2"));
        field::remove<String, String>(id, utf8(b"df3"));
    }

    public entry fun add_more_df(obj: &mut Parent) {
        let id = &mut obj.id;
        field::add<String, String>(id, utf8(b"df4"), utf8(b"df4"));
        field::add<String, String>(id, utf8(b"df5"), utf8(b"df5"));
        field::add<String, String>(id, utf8(b"df6"), utf8(b"df6"));
    }
}

//# programmable --sender A --inputs @A 42
//> 0: Test::M1::parent(Input(0));
//> 1: Test::M1::child(Input(0));

//# view-object 2,1

//# view-object 2,0

//# programmable --sender A --inputs object(2,1) object(2,0) 420
//> Test::M1::add_child(Input(0), Input(1), Input(2));
//> Test::M1::mutate_child_via_parent(Input(0), Input(2));

//# view-object 2,1

//# view-object 2,0

//# create-checkpoint

//# run-graphql
fragment DynamicFieldSelect on DynamicField {
  name {
    bcs
    type {
      repr
    }
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
  parent_version_2_no_dof: object(address: "@{obj_2_1}", version: 2) {
    address
    dynamicFields {
      ...DynamicFieldsSelect
    }
  }
  parent_version_3_has_dof: object(address: "@{obj_2_1}", version: 3) {
    dynamicFields {
      ...DynamicFieldsSelect
    }
  }
  child_version_2_no_parent: object(address: "@{obj_2_0}", version: 2) {
    address
    owner {
      ... on Parent {
        parent {
          address
        }
      }
    }
  }
  # Note that the value object's parent is the field object, not the parent object that we may
  # expect
  child_version_3_has_parent: object(address: "@{obj_2_0}", version: 3) {
    owner {
      ... on Parent {
        parent {
          address
        }
      }
    }
  }
}

//# programmable --sender A --inputs object(2,1) 420
//> Test::M1::mutate_child_via_parent(Input(0), Input(1));
//> Test::M1::add_df(Input(0));

//# view-object 2,1

//# view-object 2,0

//# create-checkpoint

//# run-graphql --cursors bcs(@{obj_5_0},1) bcs(@{obj_5_0},2)
fragment DynamicFieldSelect on DynamicField {
  name {
    bcs
    type {
      repr
    }
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
  parent_version_4_show_dof_and_dfs: object(address: "@{obj_2_1}", version: 4) {
    dynamicFields {
      ...DynamicFieldsSelect
    }
  }
  parent_version_3_only_dof: object(address: "@{obj_2_1}", version: 3) {
    dynamicFields {
      ...DynamicFieldsSelect
    }
  }
  use_dof_version_3_cursor_at_parent_version_4: object(address: "@{obj_2_1}", version: 4) {
    dynamicFields(after: "@{cursor_0}") {
      ...DynamicFieldsSelect
    }
  }
  use_dof_version_4_cursor_at_parent_version_4: object(address: "@{obj_2_1}", version: 4) {
    dynamicFields(after: "@{cursor_1}") {
      ...DynamicFieldsSelect
    }
  }
  use_dof_version_3_cursor_at_parent_version_3: object(address: "@{obj_2_1}", version: 3) {
    dynamicFields(after: "@{cursor_0}") {
      ...DynamicFieldsSelect
    }
  }
  use_dof_version_4_cursor_at_version_3: object(address: "@{obj_2_1}", version: 3) {
    dynamicFields(after: "@{cursor_1}") {
      ...DynamicFieldsSelect
    }
  }
}

//# run-graphql
fragment DynamicFieldSelect on DynamicField {
  name {
    bcs
    type {
      repr
    }
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

{
  parent_version_3: object(address: "@{obj_2_1}", version: 3) {
    dynamicObjectField(name: {type: "u64", bcs: "pAEAAAAAAAA="}) {
      ...DynamicFieldSelect
    }
    dfNotAvailableYet: dynamicField(name: {type: "0x0000000000000000000000000000000000000000000000000000000000000001::string::String", bcs: "A2RmMQ=="}) {
      ...DynamicFieldSelect
    }
  }
  parent_version_4: object(address: "@{obj_2_1}", version: 4) {
    dynamicObjectField(name: {type: "u64", bcs: "pAEAAAAAAAA="}) {
      ...DynamicFieldSelect
    }
    dfAddedHere: dynamicField(name: {type: "0x0000000000000000000000000000000000000000000000000000000000000001::string::String", bcs: "A2RmMQ=="}) {
      ...DynamicFieldSelect
    }
  }
}


//# programmable --sender A --inputs object(2,1)
//> Test::M1::add_more_df(Input(0));

//# view-object 2,1

//# view-object 2,0

//# create-checkpoint

//# run-graphql --cursors bcs(@{obj_5_0},2) bcs(@{obj_5_0},3)
fragment DynamicFieldSelect on DynamicField {
  name {
    bcs
    type {
      repr
    }
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
  parent_version_4_has_4_children: object(address: "@{obj_2_1}", version: 4) {
    dynamicFields {
      ...DynamicFieldsSelect
    }
  }
  parent_version_4_paginated_on_dof_consistent: object(address: "@{obj_2_1}", version: 4) {
    dynamicFields(after: "@{cursor_0}") {
      ...DynamicFieldsSelect
    }
  }
  parent_version_5_has_7_children: object(address: "@{obj_2_1}", version: 5) {
    dynamicFields {
      ...DynamicFieldsSelect
    }
  }
  parent_version_5_paginated_on_dof_consistent: object(address: "@{obj_2_1}", version: 5) {
    dynamicFields(after: "@{cursor_1}") {
      ...DynamicFieldsSelect
    }
  }
}

//# programmable --sender A --inputs object(2,1) 420
//> Test::M1::remove_df(Input(0));

//# view-object 2,1

//# view-object 2,0

//# create-checkpoint

//# run-graphql --cursors bcs(@{obj_5_0},2) bcs(@{obj_5_0},4)
fragment DynamicFieldSelect on DynamicField {
  name {
    bcs
    type {
      repr
    }
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
  parent_version_4_has_df1_2_3: object(address: "@{obj_2_1}", version: 4) {
    dynamicFields {
      ...DynamicFieldsSelect
    }
  }
  parent_version_4_paginated_on_dof_consistent: object(address: "@{obj_2_1}", version: 4) {
    dynamicFields(after: "@{cursor_0}") {
      ...DynamicFieldsSelect
    }
  }
  parent_version_6_no_df_1_2_3: object(address: "@{obj_2_1}", version: 6) {
    dynamicFields {
      ...DynamicFieldsSelect
    }
  }
  parent_version_6_paginated_no_df_1_2_3: object(address: "@{obj_2_1}", version: 6) {
    dynamicFields(after: "@{cursor_1}") {
      ...DynamicFieldsSelect
    }
  }
}

//# run-graphql
fragment DynamicFieldSelect on DynamicField {
  name {
    bcs
    type {
      repr
    }
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

{
  parent_version_4: object(address: "@{obj_2_1}", version: 4) {
    dfAtParentVersion4: dynamicField(name: {type: "0x0000000000000000000000000000000000000000000000000000000000000001::string::String", bcs: "A2RmMQ=="}) {
      ...DynamicFieldSelect
    }
  }
  parent_version_6: object(address: "@{obj_2_1}", version: 6) {
    dfAtParentVersion6: dynamicField(name: {type: "0x0000000000000000000000000000000000000000000000000000000000000001::string::String", bcs: "A2RmMQ=="}) {
      ...DynamicFieldSelect
    }
  }
}

//# advance-clock --duration-ns 1

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# run-graphql --cursors bcs(@{obj_5_0},2) bcs(@{obj_5_0},4)
fragment DynamicFieldSelect on DynamicField {
  name {
    bcs
    type {
      repr
    }
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
  availableRange {
    first {
      sequenceNumber
    }
    last {
      sequenceNumber
    }
  }
  parent_version_4_outside_consistent_range: object(address: "@{obj_2_1}", version: 4) {
    dynamicFields {
      ...DynamicFieldsSelect
    }
  }
  parent_version_4_paginated_outside_consistent_range: object(address: "@{obj_2_1}", version: 4) {
    dynamicFields(after: "@{cursor_0}") {
      ...DynamicFieldsSelect
    }
  }
  parent_version_6_no_df_1_2_3: object(address: "@{obj_2_1}", version: 6) {
    dynamicFields {
      ...DynamicFieldsSelect
    }
  }
  parent_version_6_paginated_no_df_1_2_3: object(address: "@{obj_2_1}", version: 6) {
    dynamicFields(after: "@{cursor_1}") {
      ...DynamicFieldsSelect
    }
  }
}

//# run-graphql
fragment DynamicFieldSelect on DynamicField {
  name {
    bcs
    type {
      repr
    }
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

{
  parent_version_4: object(address: "@{obj_2_1}", version: 4) {
    dfAtParentVersion4_outside_range: dynamicField(name: {type: "0x0000000000000000000000000000000000000000000000000000000000000001::string::String", bcs: "A2RmMQ=="}) {
      ...DynamicFieldSelect
    }
  }
  parent_version_6: object(address: "@{obj_2_1}", version: 6) {
    dfAtParentVersion6: dynamicField(name: {type: "0x0000000000000000000000000000000000000000000000000000000000000001::string::String", bcs: "A2RmMQ=="}) {
      ...DynamicFieldSelect
    }
  }
}
