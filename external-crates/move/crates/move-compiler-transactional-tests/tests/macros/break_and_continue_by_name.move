//# init --edition 2024.alpha

//# publish

module 42::m {
    macro fun loop_forever<$T>($x: $T) {
        loop $x
    }

    entry fun t0() {
        // TODO fix me. This should break the outer loop.
        // loop {
        //     loop_forever!(break)
        // }
    }

    entry fun t1() {
        // TODO fix me. This should break the outer loop.
        // let x = loop {
        //     loop_forever!(break 0)
        // };
        // assert!(x == 0, 42);
    }

    entry fun t2() {
        // TODO fix me. This should continue the outer loop.
        // let mut i = 0;
        // while (i < 10) {
        //     i = i + 1;
        //     loop_forever!(continue);
        // };
    }
}

//# run 42::m::t0

//# run 42::m::t1

//# run 42::m::t2
