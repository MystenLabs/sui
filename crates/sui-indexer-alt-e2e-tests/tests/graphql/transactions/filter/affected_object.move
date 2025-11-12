// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --addresses P=0x0 --accounts A B --simulator

//# publish
module P::M {
  use sui::transfer::Receiving;

  public struct Object has key, store {
    id: UID,
    xs: u64,
  }

  public struct Wrapper has key, store {
    id: UID,
    obj: Object,
  }

  public fun new(xs: u64, ctx: &mut TxContext): Object {
    Object { id: object::new(ctx), xs }
  }

  public fun inc(o: &mut Object) {
    o.xs = o.xs + 1;
  }

  public fun destroy(o: Object) {
    let Object { id, xs: _ } = o;
    id.delete();
  }

  public fun wrap(o: Object, ctx: &mut TxContext): Wrapper {
    Wrapper { id: object::new(ctx), obj: o }
  }

  public fun incw(w: &mut Wrapper) {
    w.obj.xs = w.obj.xs + 1;
  }

  public fun unwrap(w: Wrapper): Object {
    let Wrapper { id, obj } = w;
    id.delete();
    obj
  }

  public fun receive(p: &mut Object, r: Receiving<Object>): Object {
    transfer::receive(&mut p.id, r)
  }

  public fun drop_receiving(_: Receiving<Object>) {}

  public fun transfer(to: &Object, from: Object) {
    transfer::transfer(from, to.id.to_address())
  }
}

//# programmable --sender A --inputs 1 @A
//> P::M::new(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 2 @A
//> P::M::new(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 3 @A
//> P::M::new(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 4 @A
//> P::M::new(Input(0));
//> P::M::wrap(Result(0));
//> TransferObjects([Result(1)], Input(1))

//# programmable --sender A --inputs object(2,0)
//> P::M::inc(Input(0))

//# programmable --sender A --inputs object(5,0)
//> P::M::incw(Input(0))

//# programmable --sender A --inputs object(2,0) @A
//> P::M::wrap(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs object(8,0)
//> P::M::incw(Input(0))

//# programmable --sender A --inputs object(8,0) @A
//> P::M::unwrap(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs object(3,0) object(2,0)
//> P::M::transfer(Input(0), Input(1))

//# programmable --sender A --inputs receiving(2,0)
//> P::M::drop_receiving(Input(0))

//# programmable --sender A --inputs object(4,0) receiving(2,0) @A
//> P::M::receive(Input(0), Input(1));
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender A --inputs object(3,0) receiving(2,0) @A
//> P::M::receive(Input(0), Input(1));
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender A --inputs object(2,0)
//> P::M::destroy(Input(0))

//# programmable --sender A --inputs object(3,0)
//> P::M::destroy(Input(0))

//# programmable --sender A --inputs object(4,0)
//> P::M::destroy(Input(0))

//# programmable --sender A --inputs object(5,0) @A
//> P::M::unwrap(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs object(18,0) @A
//> P::M::wrap(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs object(19,0)
//> P::M::unwrap(Input(0));
//> P::M::destroy(Result(0))

//# create-checkpoint

//# run-graphql
{
  all: transactions { ...TX }
  affect2: transactions(filter: { affectedObject: "@{obj_2_0}" }) { ...TX }
  affect3: transactions(filter: { affectedObject: "@{obj_3_0}" }) { ...TX }
  affect4: transactions(filter: { affectedObject: "@{obj_4_0}" }) { ...TX }
  # The object that was first created at transaction 5 was unwrapped at transaction 18, so that's
  # the variable we refer to it at.
  affect5: transactions(filter: { affectedObject: "@{obj_18_0}" }) { ...TX }
  senderB_empty: transactions(filter: { affectedObject: "@{obj_2_0}", sentAddress: "@{B}" }) { ...TX }
}

fragment TX on TransactionConnection {
  nodes {
    digest
  }
}
