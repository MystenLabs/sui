// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 94 --accounts A --simulator

//# programmable --sender A --inputs @A
//> 0: sui::bag::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs object(1,0) 1000
//> 0: sui::bag::add<u64, u64>(Input(0), Input(1), Input(1))

//# programmable --sender A --inputs 2000
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::transfer::public_share_object<sui::coin::Coin<sui::sui::SUI>>(Result(0));

//# programmable --sender A --inputs 3000
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::transfer::public_freeze_object<sui::coin::Coin<sui::sui::SUI>>(Result(0));

//# programmable --sender A --inputs 4000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::party::single_owner(Input(1));
//> 2: sui::transfer::public_party_transfer<sui::coin::Coin<sui::sui::SUI>>(Result(0), Result(1));

//# create-checkpoint

//# programmable --sender A --inputs object(1,0) 5000
//> 0: sui::bag::add<u64, u64>(Input(0), Input(1), Input(1))

//# create-checkpoint

//# run-graphql
{
  addressOwned: object(address: "@{obj_0_0}") {
    owner {
      __typename
      ... on AddressOwner { address { address } }
    }
  }

  objectOwned: object(address: "@{obj_2_0}") {
    owner {
      __typename
      ... on ObjectOwner { address { address } }
    }
  }

  shared: object(address: "@{obj_3_0}") {
    owner {
      __typename
      ... on Shared { initialSharedVersion }
    }
  }

  immutable: object(address: "@{obj_4_0}") {
    owner {
      __typename
    }
  }

  partyOwned: object(address: "@{obj_5_0}") {
    owner {
      __typename
      ... on ConsensusAddressOwner {
        startVersion
        address { address }
      }
    }
  }
}

//# run-graphql
{ # Fetching the object that owns this object -- defaults to the latest
  # version, if looking at the latest version of the child.
  firstChild: object(address: "@{obj_2_0}") {
    owner {
      ... on ObjectOwner {
        address {
          asObject {
            asMoveObject {
              contents { json }
            }
          }
        }
      }
    }
  }

  secondChild: object(address: "@{obj_7_0}") {
    owner {
      ... on ObjectOwner {
        address {
          asObject {
            asMoveObject {
              contents { json }
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{ # Load parent after time travel (parent has also time traveled)
  object(address: "@{obj_2_0}", atCheckpoint: 1) {
    owner {
      ... on ObjectOwner {
        address {
          asObject {
            asMoveObject {
              contents { json }
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{ # Load parent with a root version bound (parent is also bounded)
  object(address: "@{obj_2_0}", rootVersion: 3) {
    owner {
      ... on ObjectOwner {
        address {
          asObject {
            asMoveObject {
              contents { json }
            }
          }
        }
      }
    }
  }

}
