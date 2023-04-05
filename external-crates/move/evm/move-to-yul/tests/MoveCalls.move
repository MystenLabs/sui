// Tests Move functions calling other Move functions, including generic ones which are then specialized.
#[evm_contract]
module 0x2::M {
  #[callable]
  fun f(x: u64): u64 {
    let (_, y) = g(x);
    h(y - 1)
  }

  fun h(x: u64): u64 {
    k(x) + 1
  }

  fun k<T>(x: T): T { x }

  fun g(x: u64): (u64, u64) { (x, x) }
}
