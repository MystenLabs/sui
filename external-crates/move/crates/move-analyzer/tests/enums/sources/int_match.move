module Enums::int_match {

    public fun int_match(int_param: u64) {
        match (int_param) {
            bound_var@ ( 7 | 42) if (*bound_var < 42) => bound_var,
            another_var => another_var + 42,
        };
    }
}
