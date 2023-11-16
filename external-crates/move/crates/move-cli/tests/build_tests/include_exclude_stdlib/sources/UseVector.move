module 0x1::Example {
  use std::vector;

  public fun f(addrs: &vector<address>): address {
    vector::borrow(addrs, 0)
  }
}
