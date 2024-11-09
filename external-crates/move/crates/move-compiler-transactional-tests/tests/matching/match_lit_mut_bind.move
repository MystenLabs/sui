//# init --edition 2024.beta

//# publish
module 0x42::m {
    public fun test() {
        let x = 10;
        let y = match (x) {
            0 => 10,
            mut x => {
                x = x + 10;
                x
            }
        };
        assert!(y == 20);
        let y = match (x) {
            0 => 10,
            mut y => {
                y = y + 10;
                y
            }
        };
        assert!(y == 20);
    }
}

//# run 0x42::m::test
