// tests that `new_currency` can only be called by internal structs.

module a::m {
  use a::other::External;
  use sui::coin_registry;
  use sui::object::UID;

  struct Internal has key {
    id: UID,
  }

  struct InternalGeneric<phantom T> has key {
    id: UID,
  }

  public fun t1<T: key>() {
    coin_registry::new_currency<External>();
  }

  public fun t2<T: key>() {
    coin_registry::new_currency<Internal>();
  }

  public fun t3() {
    coin_registry::new_currency<InternalGeneric<External>>();
  }
}

module a::other {
  use sui::object::UID;

  struct External has key {
    id: UID,
  }
}

module sui::object {
  struct UID has store {
    id: address,
  }
}

module sui::coin_registry {
  public fun new_currency<T: /* internal */ key>() { abort 0 }
}

module sui::coin {
  use sui::object::UID;

  struct Coin<phantom T> has key {
    id: UID,
  }
}
