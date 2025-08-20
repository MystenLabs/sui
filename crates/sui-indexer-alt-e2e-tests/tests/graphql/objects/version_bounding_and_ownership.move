// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator --addresses P0=0x0 P1=0x0

// Paginating the owned objects of an object fetched at a specific version is
// not supported, but we do support paginating the owned objects of an object
// that is latest as of a particular checkpoint. At a surface level, this is
// because of a limitation in our indexing, but this is a symptom of a deeper
// reason:
//
// - The index used to track object ownership tracks state at checkpoint boundaries.
// - Objects are versioned (every update to an object bumps its version), and
//   an object can transition through multiple versions in one checkpoint.
//
// This means that if you are looking at an object at a specific version, that
// version might not be at a checkpoint boundary, and so the ownership index
// cannot tell you which objects it owned at that version.
//
// The deeper reason for this limitation is that transferring an object A to
// another object B only modifies A, not B (it is A's "owner" field that has
// been updated).

//# programmable --sender A --inputs 100
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::object::id_address<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 2: TransferObjects([Result(0)], Result(1))

//# programmable --sender A --inputs 200
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::object::id_address<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 2: TransferObjects([Result(0)], Result(1))

//# create-checkpoint

//# programmable --sender A --inputs 300
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::object::id_address<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 2: TransferObjects([Result(0)], Result(1))

//# programmable --sender A --inputs 400 500
//> 0: SplitCoins(Gas, [Input(0), Input(1)]);
//> 1: sui::object::id_address<sui::coin::Coin<sui::sui::SUI>>(NestedResult(0,0));
//> 2: TransferObjects([NestedResult(0,1)], Result(1));
//> 3: sui::object::id_address<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 4: TransferObjects([NestedResult(0,0)], Result(3))

//# programmable --sender A --inputs 600 @0x2
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# publish --upgradeable --sender A
module P0::M {
  public fun foo(): u64 { 42 }
}

//# upgrade --package P0 --upgrade-capability 8,1 --sender A
module P1::M {
  public fun foo(): u64 { 43 }
}

//# create-checkpoint

//# run-graphql
{ # Error - Cannot query owned objects on an object fetched at a specific version
  object(address: "@{obj_0_0}", version: 2) {
    objects { nodes { address } }
  }
}

//# run-graphql
{ # Another variant of the above.
  object(address: "@{obj_0_0}") {
    objectAt(version: 2) {
      objects { nodes { address } }
    }
  }
}

//# run-graphql
{ # Error - Object fetched with rootVersion cannot query owned objects
  object(address: "@{obj_0_0}", rootVersion: 3) {
    objects { nodes { address } }
  }
}

//# run-graphql
{ # Error - Object fetched at a specific version via a nested query cannot query owned objects
  transactionEffects(digest: "@{digest_4}") {
    gasEffects {
      gasObject {
        objects { nodes { address } }
      }
    }
  }
}

//# run-graphql
{ # Error - Object version pagination does not support nested owned object queries
  object(address: "@{obj_0_0}") {
    objectVersionsBefore {
      nodes {
        objects { nodes { address } }
      }
    }
  }
}

//# run-graphql
{ # Error - Package at a specific version cannot query owned objects
  package(address: "0x2", version: 1) {
    objects { nodes { address } }
  }
}

//# run-graphql
{ # Variant of the above
  package(address: "0x2") {
    packageAt(version: 1) {
      objects { nodes { address } }
    }
  }
}

//# run-graphql
{ # Error - Package via its publishing transaction cannot query owned objects
  package(address: "@{obj_8_0}") {
    previousTransaction {
      effects {
        objectChanges {
          nodes {
            outputState {
              asMovePackage {
                objects { nodes { address } }
              }
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{ # Error - Package version pagination does not support nested owned object queries
  package(address: "@{obj_9_0}") {
    packageVersionsBefore {
      nodes {
        objects { nodes { address } }
      }
    }
  }
}

//# run-graphql
{ # Success - Object at specific checkpoint (including latest) can query owned objects

  # Should show objects owned at checkpoint 1 (first 2 transferred coins)
  cp1: checkpoint(sequenceNumber: 1) {
    query {
      object(address: "@{obj_0_0}") {
        objects { nodes { contents { json } } }
      }
    }
  }

  atCp1: object(address: "@{obj_0_0}", atCheckpoint: 1) {
    objects { nodes { contents { json } } }
  }

  # At a later checkpoint, should show more coins.
  cp2: checkpoint(sequenceNumber: 2) {
    query {
      object(address: "@{obj_0_0}") {
        objects { nodes { contents { json } } }
      }
    }
  }

  atCp2: object(address: "@{obj_0_0}", atCheckpoint: 2) {
    objects { nodes { contents { json } } }
  }

  latest: object(address: "@{obj_0_0}") {
    objects { nodes { contents { json } } }
  }

  # The ability to perform ownership queries is preserved by nesting.
  nested: object(address: "@{obj_0_0}") {
    objects {
      nodes {
        contents { json }
        objects {
          nodes {
            contents { json }
          }
        }
      }
    }
  }

  # Certain nested queries reset the "root version" bound -- like this one
  # which finds the latest version of the object.
  objectAtReset: object(address: "@{obj_0_0}", version: 2) {
    objectAt {
      objects { nodes { contents { json } } }
    }
  }

  # Querying the transaction's sender gives you its state at the current
  # checkpoint, which means it resets the "root version" bound.
  prevTxReset: object(address: "@{obj_0_0}", rootVersion: 3) {
    previousTransaction {
      sender {
        objects {
          nodes {
            contents { json }
            objects {
              nodes {
                contents { json }
              }
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{ # Success - these successful cases also apply to packages
  latest: package(address: "0x2") {
    objects { nodes { contents { json } } }
  }

  atCp1: package(address: "0x2", atCheckpoint: 1) {
    objects { nodes { contents { json } } }
  }

  cp1: checkpoint(sequenceNumber: 1) {
    query {
      package(address: "0x2") {
        objects { nodes { contents { json } } }
      }
    }
  }
}
