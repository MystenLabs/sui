module 0x42::M {

    fun func1() {
        let x = true;
        if (x) {
            false
        } else {
            true
        };

        if (x) {
            true
        } else {
            false
        };

        if (foo()) true else false;
        if (foo()) (true) else (false);
    }

    fun foo(): bool {
        true
    }
}