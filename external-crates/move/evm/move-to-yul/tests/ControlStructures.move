// Tests basic control structures.
#[evm_contract]
module 0x2::M {
  #[callable]
  fun h1(x: u64): u64 {
    if (x > 0) 1 else 2
  }

  #[callable]
  fun h2(x: u64) {
    if (x > 0) abort(1)
  }

  #[callable]
  fun f(x: u64) {
    while (x > 0) {
      if (x % 2 == 0) { x = x + 1 } else { x = x - 2 }
    }
  }

  #[callable]
  fun g(x: u64) {
    loop {
        if (x >= 1) { x = x - 1; continue };
        if (x == 0) break;
    }
  }
}
