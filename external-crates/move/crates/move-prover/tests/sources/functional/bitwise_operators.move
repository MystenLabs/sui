// exclude_for: cvc5
address 0x123 {

  module M {

    public fun foo(): u64 {
      let a = A {
        f1: 1
      };
      let b = B {
        a: a
      };
      let c = C {
        a: 1,
        b: b.a.f1
      };
      (a.f1 & 0) | (b.a.f1 & 1) | (c.b ^ 2)
    }

    spec foo {
      pragma bv_ret=b"0";
      ensures result == (((((1 as u64) & (0 as u64)) as u64) | (((1 as u64) & (1 as u64)) as u64) as u64) | (((1 as u64) ^ (2 as u64)) as u64) as u64);
    }

    struct A<T> has copy, drop {
      f1: T
    }
    spec A{
      pragma bv=b"0";
    }

    struct B<T> has drop {
      a: A<T>
    }

    struct C has drop {
      a: u64,
      b: u64
    }
    spec C {
      pragma bv=b"1";
    }

    public fun foo_generic<T>(i: T): T {
      i
    }

    spec foo_generic {
      pragma bv=b"0"; // This is necessary because the calling function may not infer the number operation
      pragma bv_ret=b"0"; // This is necessary because the calling function may not infer the number operation
    }

    public fun test(i: u8): u8 {
      let x1 = foo_generic(i);
      x1 ^ x1
    }

    spec test {
      ensures result == (0 as u8);
    }

    public fun bv_and(n: u64, e: u64): u64 {
      if (e == 0) {
        1
      } else {
        n & bv_and(n, e - 1)
      }
    }
    spec bv_and {
      pragma opaque;
      pragma bv=b"0,1";
      pragma bv_ret=b"0";
      ensures result == spec_bv_and(n, e);
    }

    spec fun spec_bv_and(n: u64, e: u64): u64 {
      if (e == (0 as u64)) { int2bv((1 as u64)) } else { n & spec_bv_and(n, e - int2bv((1 as u64))) }
    }

  }

}
