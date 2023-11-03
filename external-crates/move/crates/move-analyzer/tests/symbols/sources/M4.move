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

        let tmp = 7;

        while (tmp > 0) {
            let tmp2 = 1;
            {
                let tmp = tmp;
                tmp2 = tmp - tmp2;
            };
            tmp = tmp2;
        };

        tmp
    }

    fun loop_loop(): u64 {

        let tmp = 7;

        loop {
            let tmp2 = 1;
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

}
