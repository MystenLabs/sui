module 0x42::m {
    public enum Temperature {
       Fahrenheit(u16),
       Celsius { temp: u16 },
       Unknown
    }

    public enum EnumWithPhantom<phantom T> {
      Variant(u64)
    }

    public enum Action<NextAction> has copy, drop {
       Done,
       Left(NextAction),
       Right(NextAction),
       Jump { height: u16, then: NextAction }
    }

    public enum Expression {
       Done,
       Add,
       Mul,
       Num(u64),
    }
}
