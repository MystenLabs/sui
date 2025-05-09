// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

//# publish
module P::M {
  use sui::coin::Coin;
  use sui::sui::SUI;

  public struct Wrapper has key, store {
    id: UID,
    coin: Coin<SUI>,
  }

  public fun wrap(coin: Coin<SUI>, ctx: &mut TxContext): Wrapper {
    Wrapper { id: object::new(ctx), coin }
  }

  public fun unwrap(wrapper: Wrapper): Coin<SUI> {
    let Wrapper { id, coin } = wrapper;
    id.delete();
    coin
  }
}

//# create-checkpoint

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{ # Object created
  object(address: "@{obj_3_0}") { version }
}

//# create-checkpoint

//# run-graphql
{ # Object not touched
  object(address: "@{obj_3_0}") { version }
}

//# programmable --sender A --inputs 43 object(3,0)
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: MergeCoins(Input(1), [Result(0)])

//# create-checkpoint

//# run-graphql
{ # Object modified
  object(address: "@{obj_3_0}") { version }
}

//# programmable --sender A --inputs 44 object(3,0)
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: MergeCoins(Input(1), [Result(0)])

//# programmable --sender A --inputs 45 object(3,0)
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: MergeCoins(Input(1), [Result(0)])

//# create-checkpoint

//# run-graphql
{ # Object modified twice in one checkpoint
  object(address: "@{obj_3_0}") { version }
}

//# programmable --sender A --inputs object(3,0) @A
//> 0: P::M::wrap(Input(0));
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{ # Object wrapped
  object(address: "@{obj_3_0}") { version }
}

//# programmable --sender A --inputs object(15,0) @A
//> 0: P::M::unwrap(Input(0));
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{ # Object unwrapped
  object(address: "@{obj_3_0}") { version }
}

//# programmable --sender A --inputs object(3,0)
//> 0: MergeCoins(Gas, [Input(0)])

//# create-checkpoint

//# run-graphql
{ # Object deleted
  object(address: "@{obj_3_0}") { version }
}

//# run-graphql
{ # Querying at the checkpoint before the object was created should return no
  # result.
  beforeCreate: object(address: "@{obj_3_0}", atCheckpoint: 1) { version }

  # The object was created at this version, so it should show up.
  atCreate: object(address: "@{obj_3_0}", atCheckpoint: 2) { version }

  # The object was not modified in this checkpoint but it still exist, so it
  # should return the same version as before.
  noModification: object(address: "@{obj_3_0}", atCheckpoint: 3) { version }

  # Expect the object's version to be bumped in this checkpoint, because it was
  # modified.
  afterModification: object(address: "@{obj_3_0}", atCheckpoint: 4) { version }

  # This checkpoint includes two transactions that modified the object, so its
  # version is bumped twice.
  afterMultipleModifications: object(address: "@{obj_3_0}", atCheckpoint: 5) { version }

  # Wrapping the object hides it.
  afterWrap: object(address: "@{obj_3_0}", atCheckpoint: 6) { version }

  # And unwrapping it makes it visible again, at a bumped version.
  afterUnwrap: object(address: "@{obj_3_0}", atCheckpoint: 7) { version }

  # Deleting the object makes it disappear for good.
  afterDelete: object(address: "@{obj_3_0}", atCheckpoint: 8) { version }
}
