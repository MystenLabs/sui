module 0x42::excessive_nesting_false_negative {

    #[allow(unused_variable)]
    fun nested_in_let(x: u64) {
        let y = if (x > 0) {
            if (x > 10) {
                if (x > 20) {
                    if (x > 30) { // Nested in let binding
                        x + 1
                    } else { x }
                } else { x }
            } else { x }
        } else { x };
    }
}
