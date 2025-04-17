// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses P=0x0 --simulator

//# programmable --sender A --inputs 42
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: MergeCoins(Gas, [Result(0)])

//# publish
module P::M {
  public struct Outer has key, store {
    id: UID,
    inner: Option<Inner>,
  }

  public struct Inner has key, store {
    id: UID,
    x: u64
  }

  public fun outer(ctx: &mut TxContext): Outer {
    Outer {
      id: object::new(ctx),
      inner: option::none(),
    }
  }

  public fun inner(ctx: &mut TxContext): Inner {
    Inner {
      id: object::new(ctx),
      x: 0,
    }
  }

  public fun inc(inner: &mut Inner) {
    inner.x = inner.x + 1;
  }

  public fun wrap(outer: &mut Outer, inner: Inner) {
    outer.inner.fill(inner);
  }

  public fun unwrap(outer: &mut Outer): Inner {
    outer.inner.extract()
  }

  public fun burn(inner: Inner) {
    let Inner { id, x: _ } = inner;
    id.delete();
  }
}

//# programmable --sender A --inputs @A
//> 0: P::M::outer();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: P::M::inner();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs object(4,0)
//> 0: P::M::inc(Input(0))

//# programmable --sender A --inputs object(3,0) object(4,0) @B
//> 0: TransferObjects([Input(0), Input(1)], Input(2))

//# programmable --sender B --inputs object(3,0) object(4,0)
//> 0: P::M::wrap(Input(0), Input(1))

//# programmable --sender B --inputs object(3,0) @B
//> 0: P::M::unwrap(Input(0));
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender B --inputs object(4,0)
//> 0: P::M::burn(Input(0))

//# create-checkpoint

//# run-graphql
{ # A simple transaction where only gas has been modified.
  transactionEffects(digest: "@{digest_1}") {
    objectChanges {
      edges {
        cursor
        node {
          address
          inputState { address version digest }
          outputState { address version digest }
          idCreated
          idDeleted
        }
      }
    }
  }
}

//# run-graphql
{ # A publish transaction
  transactionEffects(digest: "@{digest_2}") {
    objectChanges {
      edges {
        cursor
        node {
          address
          inputState { address version digest }
          outputState { address version digest }
          idCreated
          idDeleted
        }
      }
    }
  }
}

//# run-graphql
{ # Transactions that create Move objects
  outer: transactionEffects(digest: "@{digest_3}") {
    objectChanges {
      edges {
        cursor
        node {
          address
          inputState { address version digest }
          outputState { address version digest }
          idCreated
          idDeleted
        }
      }
    }
  }

  inner: transactionEffects(digest: "@{digest_4}") {
    objectChanges {
      edges {
        cursor
        node {
          address
          inputState { address version digest }
          outputState { address version digest }
          idCreated
          idDeleted
        }
      }
    }
  }
}

//# run-graphql
{ # Mutating an object
  transactionEffects(digest: "@{digest_5}") {
    objectChanges {
      edges {
        cursor
        node {
          address
          inputState { address version digest }
          outputState { address version digest }
          idCreated
          idDeleted
        }
      }
    }
  }
}

//# run-graphql
{ # Transfers look like mutations
  transactionEffects(digest: "@{digest_6}") {
    objectChanges {
      edges {
        cursor
        node {
          address
          inputState { address version digest }
          outputState { address version digest }
          idCreated
          idDeleted
        }
      }
    }
  }
}

//# run-graphql
{ # Wraps look like deletions, except for the id fields
  transactionEffects(digest: "@{digest_7}") {
    objectChanges {
      edges {
        cursor
        node {
          address
          inputState { address version digest }
          outputState { address version digest }
          idCreated
          idDeleted
        }
      }
    }
  }
}

//# run-graphql
{ # Unwraps look like creations, except for the id fields
  transactionEffects(digest: "@{digest_8}") {
    objectChanges {
      edges {
        cursor
        node {
          address
          inputState { address version digest }
          outputState { address version digest }
          idCreated
          idDeleted
        }
      }
    }
  }
}

//# run-graphql
{ # Deletions cause the content to disappear, and also affects the id fields
  transactionEffects(digest: "@{digest_9}") {
    objectChanges {
      edges {
        cursor
        node {
          address
          inputState { address version digest }
          outputState { address version digest }
          idCreated
          idDeleted
        }
      }
    }
  }
}
