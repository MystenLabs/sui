// NB: Do _not_ change the number of lines in this file. Any changes to the
// number of lines in this file may break the expected output of this test.

//# init --edition 2024.beta

//# publish
module 0x42::m {
    macro fun a() {
        abort
    }

    macro fun calls_a() {
        a!()
    }

    entry fun t_a() {
        a!() // assert should point to this line
    }

    entry fun t_calls_a() {
        calls_a!() // assert should point to this line
    }
}

//# run 0x42::m::t_a

//# run 0x42::m::t_calls_a
