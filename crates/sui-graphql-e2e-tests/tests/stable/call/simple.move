// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses Test=0x0 A=0x42 --simulator --custom-validator-account --reference-gas-price 234 --default-gas-price 1000

//# publish
module Test::M1 {
    use sui::coin::Coin;

    public struct Object has key, store {
        id: UID,
        value: u64,
    }

    fun foo<T: key, T2: drop>(_p1: u64, value1: T, _value2: &Coin<T2>, _p2: u64): T {
        value1
    }

    public entry fun create(value: u64, recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Object { id: object::new(ctx), value },
            recipient
        )
    }
}

//# run Test::M1::create --args 0 @A --gas-price 1000

//# run Test::M1::create --args 0 @validator_0

//# view-object 0,0

//# view-object 2,0

//# view-object 3,0

//# create-checkpoint 4

//# view-checkpoint

//# advance-epoch 6

//# view-checkpoint

//# run-graphql
{
  checkpoint {
    sequenceNumber
  }
}

//# create-checkpoint

//# view-checkpoint

//# run-graphql
{
  checkpoint {
    sequenceNumber
  }
}

//# run-graphql --show-usage --show-headers --show-service-version
{
  checkpoint {
    sequenceNumber
  }
}

//# view-checkpoint

//# advance-epoch

// Demonstrates using variables
// If the variable ends in _opt, this is the optional variant

//# run-graphql
{
  address(address: "@{A}") {
    objects {
      edges {
        node {
          address
          digest
          owner {
              __typename
          }
        }
      }
    }
  }
}

//# run-graphql
{
  address(address: "@{Test}") {
    objects {
      edges {
        node {
          address
          digest
          owner {
              __typename
          }
        }
      }
    }
  }
  second: address(address: "@{A}") {
    objects {
      edges {
        node {
          address
          digest
          owner {
              __typename
          }
        }
      }
    }
  }

  val_objs: address(address: "@{validator_0}") {
    objects {
      edges {
        node {
          address
          digest
          owner {
              __typename
          }
        }
      }
    }
  }

  object(address: "@{obj_2_0}") {
    version
    owner {
      __typename
      ... on AddressOwner {
        owner {
          address
        }
      }
    }
  }

}

//# run-graphql
{
  epoch {
    validatorSet {
      activeValidators {
        nodes {
          address {
            address
          }
        }
      }
    }
  }
  address(address: "@{validator_0}") {
    address
  }
}

//# run-graphql
# Since we set it at the init, it should be the same as 234
{
  epoch {
    referenceGasPrice
  }
}

//# run Test::M1::create --args 0 @A --gas-price 999

//# run Test::M1::create --args 0 @A --gas-price 1000

//# run Test::M1::create --args 0 @A --gas-price 235
