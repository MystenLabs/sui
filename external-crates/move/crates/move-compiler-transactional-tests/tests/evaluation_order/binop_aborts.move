//# publish
module 0x42::m {
    public fun aborter(x: u64): u64 {
        abort x
    }

    public fun add3(x: u64, y: u64, z: u64): u64 {
        x + y + z
    }
}

// All of these should abort

//# run
module 0x43::test0 {
    // abort with 2
    public fun main(): u64 {
        if (false) {0x42::m::aborter(1)} else {abort 2} + {abort 3; 0}
    }
}

//# run
module 0x44::test1 {
    // abort with 1
    public fun main(): u64 {
        let x = 1;
        0x42::m::aborter(x) + {x = x + 1; 0x42::m::aborter(x + 100); x} + x
    }
}

//# run
module 0x45::test2 {
    // aborts with bad math
    public fun main(): u8 {
        abort (1u64 - 10u64)
    }
}

//# run
module 0x46::test3 {
    // aborts with bad math
    public fun main(): u8 {
        {250u8 + 50u8} + {abort 55; 5u8}
    }
}

//# run
module 0x47::test4 {
    // aborts with 0
    public fun test(): u64 {
        0x42::m::add3(abort 0, {abort 14; 0}, 0)
    }
}

//# run
module 0x48::test5 {
    //abort with 0
    public fun test(): u64 {
        (abort 0) + {(abort 14); 0} + 0
    }
}

//# run
module 0x49::test37 {
    // aborts with bad math
    public fun main(): u8 {
        {250u8 + 50u8} + {return 55; 5u8}
    }
}

