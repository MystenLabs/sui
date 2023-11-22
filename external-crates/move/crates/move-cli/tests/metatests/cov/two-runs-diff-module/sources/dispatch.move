module 0x42::dispatch {
    entry fun test(x: u8) {
        if (x == 0) 0x42::M1::test() else 0x42::M2::test()
    }
}
