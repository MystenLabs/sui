module 0x42::Test {
    struct X has drop {}

    // test basic, valid vector literals
    fun none() {
        (vector[]: vector<bool>);
    }


    fun one() {
        (vector[0]: vector<u8>);
        (vector[0]: vector<u64>);
        (vector[0]: vector<u128>);
        (vector[@0]: vector<address>);
        (vector[X{}]: vector<X>);

        (vector[vector[]]: vector<vector<address>>);
        (vector[vector[vector[]]]: vector<vector<vector<address>>>);
    }

    fun many() {
        (vector[0, 1, 2]: vector<u8>);
        (vector[0, 1, 2]: vector<u64>);
        (vector[0, 1, 2]: vector<u128>);
        (vector[@0, @1]: vector<address>);
        (vector[X{}, X{}]: vector<X>);

        (vector[vector[], vector[]]: vector<vector<address>>);
        (vector[vector[vector[], vector[]], vector[]]: vector<vector<vector<address>>>);
    }
}
