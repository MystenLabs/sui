  module 0x5::l {
      const A: u64 = 1;
      const B: vector<u64> = vector[1,2,3];

      public struct X has drop {
          x: u64, 
          y: bool,
      }

      public struct Y<A, B> has drop {
          x: A,
          y: B,
      }

      #[test]
      fun test() {
          f()
      }
  
      fun f() {
          let x = k() + 1;
          if (x > 0) g(x)
      }

      fun k(): u64 {
          1
      }
  
      fun g(x: u64) {
          let y = x as u8;
          let _ = B;
          let x = X { x: A, y: true };
          let j = Y { x, y: true };
          h(i(j));
          h(y)
      }
  
      fun h(_y: u8) { }
  
      fun i<A: drop, B: drop>(y: Y<A, B>): u8 { 
          let Y { x: _, y: _ } = y;
          let x = &1;
          let h = *x;
          let j = &mut 1;
          *j = 2;
          *j
      }
  }
