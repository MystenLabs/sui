address 0x42 {
module m {

    fun t0(): u64 {
        let x = 0;
        while (x < 10) {
            if (x < 5) {
                x = x + 1;
            } else {
                return x
            }
        };
        x
    }

    fun t1(): u64 {
        let x = 0;
        while (x < 10) {
            if (x < 5) {
                x = x + 1;
            } else {
                abort x
            }
        };
        x
    }

    fun t2(x: u64): u64 {
         let y = if (x  > 10) { x - 10 } else { return 10 };
         y * y
    }

    fun t3(x: u64): u64 {
         let y = if (x  > 10) { x - 10 } else { abort 10 };
         y * y
    }

    fun t4(x: u64): u64 {
        if ((x > 10) ||  return x ) { x * x } else { x }
    }
    
    fun t5(x: u64): u64 {
        if ((x > 10) ||  abort x ) { x * x } else { x }
    }

}
}
