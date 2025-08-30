// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# programmable --sender A --inputs @A
//> 0: sui::bag::new();
//> TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::object_bag::new();
//> TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs 100 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 200 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 300 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs object(1,0) 42 999
//> sui::bag::add<u64, u64>(Input(0), Input(1), Input(2))

//# create-checkpoint

//# programmable --sender A --inputs object(1,0) 43 object(3,0)
//> sui::bag::add<u64, sui::coin::Coin<sui::sui::SUI>>(Input(0), Input(1), Input(2))

//# programmable --sender A --inputs object(2,0) 44 object(4,0)
//> sui::object_bag::add<u64, sui::coin::Coin<sui::sui::SUI>>(Input(0), Input(1), Input(2))

//# create-checkpoint

//# programmable --sender A --inputs object(1,0) 45 object(5,0)
//> sui::bag::add<u64, sui::coin::Coin<sui::sui::SUI>>(Input(0), Input(1), Input(2))

//# create-checkpoint

//# run-graphql --cursors bcs(42u64)
{ # Successfully fetch a dynamic field with primitive value
  object(address: "@{obj_1_0}") {
    address
    asMoveObject {
      dynamicField(name: { type: "u64", bcs: "@{cursor_0}" }) {
        name {
          type { repr }
          json
        }

        value {
          ... on MoveValue {
            type { repr }
            json
          }
        }
      }
    }
  }
}

//# run-graphql --cursors bcs(43u64)
{ # Successfully fetch a dynamic field with object value (wrapped)
  object(address: "@{obj_1_0}") {
    address
    asMoveObject {
      dynamicField(name: { type: "u64", bcs: "@{cursor_0}" }) {
        name {
          type { repr }
          json
        }

        value {
          ... on MoveValue {
            type { repr }
            bcs
          }
        }
      }
    }
  }
}

//# run-graphql --cursors bcs(44u64)
{ # Successfully fetch a dynamic object field
  object(address: "@{obj_2_0}") {
    address
    asMoveObject {
      dynamicObjectField(name: { type: "u64", bcs: "@{cursor_0}" }) {
        name {
          type { repr }
          json
        }

        value {
          ... on MoveObject {
            address
            version

            contents {
              type { repr }
              json
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{ # Failed cast of a regular MoveObject (not a dynamic field)
  object(address: "@{obj_5_0}") {
    address
    version

    asMoveObject {
      contents {
        type { repr }
        json
      }

      asDynamicField {
        address
        name { json }
        value {
          ... on MoveValue { json }
          ... on MoveObject { address }
        }
      }
    }
  }
}

//# run-graphql
{ # Failed cast of a MovePackage
  object(address: "0x2") {
    address

    asMoveObject {
      asDynamicField {
        name { json }
      }
    }
  }
}

//# run-graphql
{ # Verify wrapped object is not directly accessible
  object(address: "@{obj_3_0}") {
    address
    version
  }
}

//# run-graphql
{ # Verify dynamic object field's value is directly accessible
  object(address: "@{obj_4_0}") {
    address
    version
    asMoveObject {
      contents {
        type { repr }
        json
      }
    }
  }
}

//# run-graphql
{ # Query the dynamic field objects directly by their addresses

  df42: object(address: "@{obj_6_0}") { ...asDF }
  df43: object(address: "@{obj_8_0}") { ...asDF }
  dof44: object(address: "@{obj_9_0}") { ...asDF }
  df45: object(address: "@{obj_11_0}") { ...asDF }
}

fragment asDF on Object {
  asMoveObject {
    asDynamicField {
      name { json }
      value { ... on MoveValue { json } }
    }
  }
}

//# run-graphql --cursors bcs(42u64) bcs(43u64) bcs(45u64)
{ # Dynamic field ownership respects parent versioning
  objectVersions(address: "@{obj_1_0}") {
    nodes {
      version

      asMoveObject {
        df42: dynamicField(name: { type: "u64", bcs: "@{cursor_0}" }) { ...DF }
        df43: dynamicField(name: { type: "u64", bcs: "@{cursor_1}" }) { ...DF }
        df45: dynamicField(name: { type: "u64", bcs: "@{cursor_2}" }) { ...DF }
      }
    }
  }
}

fragment DF on DynamicField {
  name { json }
  value { ... on MoveValue { json } }
}

//# run-graphql --cursors bcs(42u64) bcs(43u64) bcs(45u64)
{ # Dynamic field ownership also respects checkpoint bounding
  atCp1: object(address: "@{obj_1_0}", atCheckpoint: 1) { ...Parent }
  atCp2: object(address: "@{obj_1_0}", atCheckpoint: 2) { ...Parent }
  atCp3: object(address: "@{obj_1_0}", atCheckpoint: 3) { ...Parent }
}

fragment Parent on Object {
  version
  asMoveObject {
    df42: dynamicField(name: { type: "u64", bcs: "@{cursor_0}" }) { ...DF }
    df43: dynamicField(name: { type: "u64", bcs: "@{cursor_1}" }) { ...DF }
    df45: dynamicField(name: { type: "u64", bcs: "@{cursor_2}" }) { ...DF }
  }
}

fragment DF on DynamicField {
  name { json }
  value { ... on MoveValue { json } }
}
