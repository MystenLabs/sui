// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 108 --accounts A --simulator --addresses P=0x0

//# programmable --sender A --inputs 1000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{
  # Address present in the tx as a newly created object → ObjectChange variant.
  changed: address(address: "@{obj_1_0}") {
    asTransactionObject(transactionDigest: "@{digest_1}") {
      __typename
      ... on ObjectChange {
        idCreated
        outputState { address }
      }
    }
  }

  # Same resolver called directly on an Object (an IAddressable implementor).
  objectDirect: object(address: "@{obj_1_0}") {
    asTransactionObject(transactionDigest: "@{digest_1}") {
      __typename
      ... on ObjectChange {
        idCreated
        outputState { address }
      }
    }
  }

  # No `transactionDigest` argument outside subscription scope → null.
  noArg: address(address: "@{obj_1_0}") {
    asTransactionObject {
      __typename
    }
  }

  # Address not referenced by the tx → null.
  notReferenced: address(
    address: "0x000000000000000000000000000000000000000000000000000000000000dead"
  ) {
    asTransactionObject(transactionDigest: "@{digest_1}") {
      __typename
    }
  }
}

//# publish
module P::M {
    use sui::event;

    public struct TestAddressEvent has copy, drop {
        address_event_id: address,
    }

    public struct TestObject has key, store {
        id: UID,
        value: u64,
    }

    public fun create_object(value: u64, ctx: &mut TxContext): TestObject {
        TestObject { id: object::new(ctx), value }
    }

    public fun mutate_and_emit(obj: &mut TestObject, new_value: u64) {
        obj.value = new_value;
        event::emit(TestAddressEvent {
            address_event_id: object::id(obj).to_address(),
        });
    }
}

//# create-checkpoint

//# programmable --sender A --inputs 7u64 @A
//> 0: P::M::create_object(Input(0));
//> 1: TransferObjects([Result(0)], Input(1));

//# create-checkpoint

//# programmable --sender A --inputs object(6,0) 99u64
//> 0: P::M::mutate_and_emit(Input(0), Input(1));

//# create-checkpoint

//# run-graphql
{
  # `asTransactionObject` is called without a `transactionDigest`. Resolution defaults to
  # the parent event's transaction because `EffectsContents::events` anchors each yielded
  # `Event`'s scope to the parent transaction (with hydrated contents), and the resolver
  # short-circuits the fetch via `Scope::active_transaction_contents_for`.
  transactionEffects(digest: "@{digest_8}") {
    events {
      nodes {
        contents {
          extract(path: "address_event_id") {
            asAddress {
              asTransactionObject {
                __typename
                ... on ObjectChange {
                  inputState { asMoveObject { contents { json } } }
                  outputState { asMoveObject { contents { json } } }
                }
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
  # Same implicit case as above, but entering via `transaction(...).effects` instead of
  # `transactionEffects(...)`. `From<Transaction> for TransactionEffects` propagates the
  # hydrated contents into the scope, and `EffectsContents::events` anchors them on each
  # `Event` so descendants resolve against the parent transaction.
  transaction(digest: "@{digest_8}") {
    effects {
      events {
        nodes {
          contents {
            extract(path: "address_event_id") {
              asAddress {
                asTransactionObject {
                  __typename
                  ... on ObjectChange {
                    inputState { asMoveObject { contents { json } } }
                    outputState { asMoveObject { contents { json } } }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
