// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses test=0x0 --simulator

//# publish --sender A
module test::mod {
  use std::ascii::{string as ascii, String as ASCII};
  use std::string::{utf8, String as UTF8};
  use sui::dynamic_field as df;
  use sui::dynamic_object_field as dof;
  use sui::vec_map::{Self, VecMap};
  use sui::versioned::{Self, Versioned};

  public struct Foo has key, store {
    id: UID,
    bar: vector<Bar>,
  }

  public enum Bar has store, drop {
    Baz(u64),
    Qux { val: UTF8 },
    Quy { opt: Option<u16> },
  }

  public struct Quz has key, store {
    id: UID,
    map: VecMap<ASCII, u128>,
  }

  public fun example(ctx: &mut TxContext): Foo {
    Foo {
      id: object::new(ctx),
      bar: vector[
        Bar::Baz(42u64),
        Bar::Qux { val: utf8(b"hello") },
        Bar::Quy { opt: option::some(7u16) },
        Bar::Quy { opt: option::none() },
      ],
    }
  }

  public fun versioned(ctx: &mut TxContext): Versioned {
    versioned::create(1, Bar::Baz(42), ctx)
  }

  public fun upgrade(v: &mut Versioned) {
    let (_, cap) = v.remove_value_for_upgrade<Bar>();
    v.upgrade(2, Bar::Qux { val: utf8(b"upgraded") }, cap);
  }

  public fun add_df(foo: &mut Foo) {
    df::add(&mut foo.id, 43u8, 100u128) ;
  }

  public fun add_dof(foo: &mut Foo, ctx: &mut TxContext) {
    let mut map = vec_map::empty();
    map.insert(ascii(b"hello"), 101u128);
    map.insert(ascii(b"world"), 102u128);

    dof::add(&mut foo.id, 44u32, Quz {
      id: object::new(ctx),
      map,
    })
  }
}

//# programmable --sender A --inputs @A
//> 0: test::mod::example();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: test::mod::versioned();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs object(3,1)
//> 0: test::mod::upgrade(Input(0))

//# programmable --sender A --inputs object(2,0)
//> 0: test::mod::add_df(Input(0))

//# programmable --sender A --inputs object(2,0)
//> 0: test::mod::add_dof(Input(0))

//# create-checkpoint

//# run-graphql
{
  object(address: "@{obj_2_0}") {
    asMoveObject {
      contents {
        ...MV

        # First level field access
        bar: extract(path: "bar") { ...MV }

        # Second level field access
        baz: extract(path: "bar[0u64]") { ...MV }
        qux: extract(path: "bar[1u64]") { ...MV }
        quySome: extract(path: "bar[2u64]") { ...MV }
        quyNone: extract(path: "bar[3u64]") { ...MV }

        # Fields inside variants
        bazField: extract(path: "bar[0u64].0") { ...MV }
        quxField: extract(path: "bar[1u64].val") { ...MV }
        quySomeField: extract(path: "bar[2u64].opt") { ...MV }

        # Dynamic (Object) Fields
        df: extract(path: "id->[43u8]") { ...MV }
        dof: extract(path: "id=>[44u32]") { ...MV }
        dofHello: extract(path: "id=>[44u32].map['hello']") { ...MV }
        dofWorld: extract(path: "id=>[44u32].map['world']") { ...MV }

        # Doesn't exist for various reason
        outOfBounds: extract(path: "bar[4u64]") { ...MV }
        quyNoneField: extract(path: "bar[3u64].opt") { ...MV }
        quyNoneInnerField: extract(path: "bar[3u64].opt.val") { ...MV }
        outOfBoundsField: extract(path: "bar[4u64].val") { ...MV }
        fieldDoesntExist: extract(path: "iDontExist") { ...MV }
        dfDoesntExist: extract(path: "id->[43u32]") { ...MV }
        dofDoesntExist: extract(path: "id=>[44u8]") { ...MV }
        dfOnNotAnID: extract(path: "bar->[43u8]") { ...MV }
      }
    }
  }
}

fragment MV on MoveValue {
  type { repr }
  json
}

//# run-graphql
{
  versioned: objectVersions(address: "@{obj_3_1}", first: 2) {
    pageInfo {
      hasNextPage
      endCursor
    }

    nodes {
      version
      asMoveObject {
        contents {
          extract(path: "id->[version]") {
            type { repr }
            json
          }
        }
      }
    }
  }
}

//# run-graphql
{
  object(address: "@{obj_2_0}") {
    asMoveObject {
      contents {
        parseError: extract(path: "id.") { json }
      }
    }
  }
}

//# run-graphql
{
  object(address: "@{obj_2_0}") {
    asMoveObject {
      contents {
        tooDeep: extract(path: "a[b[c[d[e[f[g[h[i[j[k[l[m[n[o[p[q[r[s[t[u[v[w[x[y[z[a[b[c[d[e[f[g[h[i[j[k[l[m[n[o[p[q[r[s[t[u[v[w[x[y[z]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]") { json }
      }
    }
  }
}

//# run-graphql
{
  object(address: "@{obj_2_0}") {
    asMoveObject {
      contents {
        tooManyLoads: extract(path: "a->[b]->[c]->[d]->[e]->[f]->[g]->[h]->[i]->[j]") { json }
      }
    }
  }
}

//# run-graphql
{
  object(address: "@{obj_2_0}") {
    asMoveObject {
      contents {
        notASlice: extract(path: "vector[1u8]") { json }
      }
    }
  }
}
