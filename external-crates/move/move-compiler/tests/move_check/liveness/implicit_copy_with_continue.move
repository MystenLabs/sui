module 0x42::m {
    public fun bar() {
        abort 0
    }

    public fun foo(_: &u64) {
        abort 0
    }

    public fun a() {
        let index: u64 = 0;
        let sum = 0;
        while (index < 10) {

            if (index % 2 == 0) {
                sum = sum + index
            } else {
                index = index + 1;
                bar();
                continue
            };

            index = index + 1;
        };
        foo(&sum);
    }
}
