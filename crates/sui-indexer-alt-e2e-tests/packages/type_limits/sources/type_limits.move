// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module type_limits::type_limits;

public struct Deep0<T: store> has key, store { id: UID, inner: T }
public struct Deep1<T: store> has key, store { id: UID, inner: T }
public struct Deep2<T: store> has key, store { id: UID, inner: T }
public struct Deep3<T: store> has key, store { id: UID, inner: T }

public struct Wide<T: store, U: store, V: store, W: store> has key, store {
  id: UID,
  t: T,
  u: U,
  v: V,
  w: W,
}

public fun deep0<T: store>(inner: T, ctx: &mut TxContext): Deep0<T> {
  Deep0 { id: object::new(ctx), inner }
}

public fun deep1<T: store>(inner: T, ctx: &mut TxContext): Deep1<T> {
  Deep1 { id: object::new(ctx), inner }
}

public fun deep2<T: store>(inner: T, ctx: &mut TxContext): Deep2<T> {
  Deep2 { id: object::new(ctx), inner }
}

public fun deep3<T: store>(inner: T, ctx: &mut TxContext): Deep3<T> {
  Deep3 { id: object::new(ctx), inner }
}

public fun wide<T: store, U: store, V: store, W: store>(
  t: T,
  u: U,
  v: V,
  w: W,
  ctx: &mut TxContext,
): Wide<T, U, V, W> {
  Wide { id: object::new(ctx), t, u, v, w }
}
