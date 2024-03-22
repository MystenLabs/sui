module a::m {

    fun simple(cond: bool, x: u32) {
        if (cond) x else { x } as u32;
        while (cond) {} as u32;
        loop {} as u32;
        'l: { 0 } as u32;
        || { 0 } as u32;
        return as u32;
        loop {
            break as u32;
            continue as u32;
        };
        0 as u32 as u32;
    }

    fun ops(x: u8, y: u8) {
        x + y as u32;
        x - y as u32;
        x * y as u32;
        x / y as u32;
        x % y as u32;
        x & y as u32;
        x | y as u32;
        x ^ y as u32;
        x << y as u32;
        x >> y as u32;
        !x as u32;
    }
}
