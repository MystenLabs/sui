module 0x12::test {
    struct Example has copy, drop { i: u64 }

    const ONE: u64 = 1;

    public fun print(x: u64): u64 {
        let sum = x + ONE;
        let abc = x + 2;
        let example = Example { i: abc };

        if (sum < 10) {
          sum = sum + 1;
          sum = sum + if (sum > 11) {
            let a = 1;
            a
          } else {
            let a = 2;
            a
          };
        } else {
          if (sum == 11) {
            return 1
          };
          sum = sum + 2;
        };

        if (sum > 11) {
          abort 23
        } else {
          sum = sum + 2;
        };
        let xyz = sum;
        if (xyz < 13) {
          return xyz + 1
        } else {
          abort xyz - 10
        }
    }
}
