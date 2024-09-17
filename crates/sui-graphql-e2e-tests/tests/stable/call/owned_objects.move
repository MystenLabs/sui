// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses Test=0x0 A=0x42 --simulator

// Tests objects on address, object, and owner.
//
// - The initial query for objects under address should yield no
//   objects.
// - After object creation, the same query for address.objects should
//   now have one object
// - A query for transactions belonging to an address that also
//   supplies an address filter will combine the two filters.
// - If the two filters suggest two different addresses, they will
//   combine to form an inconsistent query, which will yield no
//   results.
// - The same query on the address as an owner should return the same
//   result
// - The same query on the address as an object should return a null
//   result, since the address is not an object

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

//# run-graphql
{
  address(address: "0x42") {
    objects {
      edges {
        node {
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
    }
  }
}

//# run Test::M1::create --args 0 @A

//# view-object 3,0

//# create-checkpoint

//# run-graphql
{
  address(address: "0x42") {
    objects {
      edges {
        node {
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
    }
  }
}

//# run-graphql
{
  address(address: "0x42") {
    objects(filter: {owner: "0x42"}) {
      edges {
        node {
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
    }
  }
}

//# run-graphql
{
  address(address: "0x42") {
    objects(filter: {owner: "0x888"}) {
      edges {
        node {
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
    }
  }
}

//# run-graphql
{
  owner(address: "0x42") {
    objects {
      edges {
        node {
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
    }
  }
}

//# run-graphql
{
  object(address: "0x42") {
    objects {
      edges {
        node {
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
    }
  }
}
