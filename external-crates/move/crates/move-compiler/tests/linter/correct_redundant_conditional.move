module 0x42::M {

    fun func1(): u64 {
        let x = 3;
        if (x > 4) {
            x = 2;
        } else {
            x = 1;
        };
        x
    }
}
