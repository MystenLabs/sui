module 0x42::loop_test {
    use std::vector;

    // This function should trigger the linter
    public fun infinite_loop() {
        while (true) {
            // This loop will run forever
        }
    }

    // This function should also trigger the linter
    public fun finite_loop() {
        let counter = 0;
        while (true) {
            if (counter == 10) {
                break
            };
            counter = counter + 1;
        }
    }

    // This function should not trigger the linter
    public fun correct_infinite_loop() {
        loop {
            // This is the correct way to write an infinite loop
        }
    }

    // This function should not trigger the linter
    public fun while_with_condition(n: u64) {
        let i = 0;
        while (i < n) {
            i = i + 1;
        }
    }

    // This function uses both `while(true)` and `loop` for comparison
    public fun mixed_loops() {
        let vec1 = vector::empty<u64>();
        let vec2 = vector::empty<u64>();

        // This should trigger the linter
        while (true) {
            if (vector::length(&vec1) == 5) break;
            vector::push_back(&mut vec1, vector::length(&vec1));
        };

        // This should not trigger the linter
        loop {
            if (vector::length(&vec2) == 5) break;
            vector::push_back(&mut vec2, vector::length(&vec2));
        };
    }
}
