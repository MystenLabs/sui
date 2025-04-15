// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses Test=0x0 --accounts A B --simulator --objects-snapshot-min-checkpoint-lag 4


// cp | object_id | owner
// ----------------------
// 1  | obj_3_0   | A
// 2  | obj_5_0   | A
// 2  | obj_6_0   | A
// All owned by B after checkpoint 3.

//# publish
module Test::M1 {
    public struct Object has key, store {
        id: UID,
        value: u64,
    }

    public entry fun create(value: u64, recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Object { id: object::new(ctx), value },
            recipient
        )
    }
}

//# run Test::M1::create --args 0 @A

//# run Test::M1::create --args 1 @A

//# create-checkpoint

//# run Test::M1::create --args 2 @A

//# run Test::M1::create --args 3 @A

//# create-checkpoint

//# run-graphql
# Fetch specific versions of objects.
{
  objects_at_version: multiGetObjects(keys: [
            {objectId: "@{obj_2_0}", version: 3},
            {objectId: "@{obj_3_0}", version: 4},
            {objectId: "@{obj_5_0}", version: 5},
            {objectId: "@{obj_6_0}", version: 6}
  ]) {
        version
        asMoveObject {
          contents {
            type {
              repr
            }
            json
          }
        }
      }
}

//# programmable --sender A --inputs object(2,0) object(3,0) object(5,0) object(6,0) @B
//> TransferObjects([Input(0), Input(1), Input(2), Input(3)], Input(4))

//# create-checkpoint

//# run-graphql
# Fetch specific versions of objects including a non existing object.
{
  objects_at_version: multiGetObjects(keys: [
      {objectId: "0x1", version: 1},
      {objectId: "@{obj_3_0}", version: 4},
      {objectId: "@{obj_5_0}", version: 5},
      {objectId: "0xabc", version: 1}
  ]) {
        version
        asMoveObject {
          contents {
            type {
              repr
            }
            json
          }
       }
    }
}

