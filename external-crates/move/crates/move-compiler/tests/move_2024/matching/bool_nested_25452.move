  module 0x0::M {
      public struct S has drop { b: bool }
      fun f(s: S): u64 {
          match (s) {
              S { b: 0 } => 1,
              _ => 2,
          }
      }
  }
