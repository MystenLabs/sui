module 0x42::M {

    fun func1(): (u64, u64) {
        let x = 5;
        let y = 10;

        // Direct self-assignment
        x = x;

        // Self-assignment within an expression (though not very meaningful in Move, included for completeness)
        y = y + 0;
        (x, y)
    }
}
