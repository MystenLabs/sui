module 0x1::bench {
    const COUNT: u64 = 10_000u64;

    public struct LargeStruct has drop {
        a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64, h: u64
    }

    fun bench_inner(): LargeStruct {
        let mut i = 0;
        let mut alloc = LargeStruct { a: 0, b: 0, c: 0, d: 0, e: 0, f: 0, g: 0, h: 0 };
        while (i < COUNT) {
            alloc = LargeStruct { a: i, b: i, c: i, d: i, e: i, f: i, g: i, h: i };
            i = i + 1;
        };
        alloc
    }

    public fun bench() {
	    let LargeStruct { a: _, b: _, c: _, d: _, e: _, f: _, g: _, h: i } = bench_inner();
        let _i = i;
    }
}
