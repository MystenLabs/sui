module Symbols::M4 {

    fun if_cond(tmp: u64): u64 {

        let tmp = tmp;

        let ret = if (tmp == 7) {
            tmp
        } else {
            let tmp = 42;
            tmp
        };

        ret
    }

    fun while_loop(): u64 {

        let mut tmp = 7;

        while (tmp > 0) {
            let mut tmp2 = 1;
            {
                let tmp = tmp;
                tmp2 = tmp - tmp2;
            };
            tmp = tmp2;
        };

        tmp
    }

    fun loop_loop(): u64 {

        let mut tmp = 7;

        loop {
            let mut tmp2 = 1;
            {
                let tmp = tmp;
                tmp2 = tmp - tmp2;
            };
            tmp = tmp2;
            if (tmp == 0) {
                break
            }
        };

        tmp
    }

}

module Symbols::M5 {

    const SOME_CONST: u64 = 7;

    public fun long_param_list(foo: u64, bar: u64, baz: u64, qux: u64) {}

    public fun short_type_param_list<TYPE1, TYPE2>() {}

    public fun long_type_param_list<TYPE1, TYPE2, TYPE3>() {}

    public fun combined_short_type_param_list<TYPE1, TYPE2>(
        foo: u64, bar: u64, baz: u64, qux: u64
    ) {}

    public fun combined_long_type_param_list<TYPE1, TYPE2, TYPE3>(
        foo: u64, bar: u64, baz: u64, qux: u64
    ) {}

    public fun stripped_types(mut opt: std::option::Option<u64>): vector<u64> {
        // hovering over `extract` should strip `std::option` from parameter type
        // `std` from the (qualified) function name
        let elem: u64 = std::option::extract(&mut opt);
        // hovering over `singleton` should strip `std` from the (qualified)
        // function name
        std::vector::singleton(elem)
    }

}
