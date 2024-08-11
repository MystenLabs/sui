module 0x42::m {
    enum Temperature {
       Fahrenheit(u16),
       Celsius { temp: u16 },
       Unknown
    }

    public(package) enum EnumWithPhantom<phantom T> {
      Variant(u64)
    }

    public enum Expression {
       Done(x: u64),
       Add { u64 },
    }
}
