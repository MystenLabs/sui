module 0x42::TestConst {

    struct T {
        x: u64,
        b: bool,
        a: address,
    }
    const INIT_VAL_U64: u64 = 40 + 2;
    const FORTY_TWO: u64 = 42;
    const INIT_VAL_BOOL: bool = true;
    const ONE: u64 = 1;
    const ADDR: address = @0x2;
    const BYTE_ARRAY: vector<u8> = vector<u8>[11, 22, 33];
    const ADDRESS_ARRAY: vector<address> = vector<address>[@0x111, @0x222, @0x333];
    const BOOL_ARRAY: vector<bool> = vector<bool>[true, false, true];

    public fun init(): T {
        T { x: 43, b: !INIT_VAL_BOOL, a: ADDR }
    }

    spec init {
        ensures result.x == INIT_VAL_U64 + 1;
        ensures !result.b;
    }

    public fun init_incorrect(): T {
        T { x: 43, b: INIT_VAL_BOOL, a: @0x1 }
    }

    spec init_incorrect {
        ensures result.x == FORTY_TWO + ONE;
        ensures !result.b;
    }

    public fun array_correct() {
        spec {
            assert BYTE_ARRAY[0] == 11 && BYTE_ARRAY[1] == 22 && BYTE_ARRAY[2] == 33;
            assert ADDRESS_ARRAY[0] == @0x111 && ADDRESS_ARRAY[1] == @0x222 && ADDRESS_ARRAY[2] == @0x333;
            assert BOOL_ARRAY[0] == true && BOOL_ARRAY[1] == false && BOOL_ARRAY[2] == true;
        };
    }

    public fun array_1_incorrect() {
        spec {
            assert BYTE_ARRAY[0] == 22;
        };
    }

    public fun array_2_incorrect() {
        spec {
            assert ADDRESS_ARRAY[0] == @0x222;
        };
    }

    public fun array_in_fun() {
        let v1 = vector<address> [@0x1, @0x2, @0x3];
        let v2 = vector<bool> [false, true, false];
        spec {
            assert v2[0] == false && v2[1] == true && v2[2] == false;
            assert v1[0] == @0x1 && v1[1] == @0x2 && v1[2] == @0x3;
        };
    }

    public fun array_in_fun_incorrect() {
        let v1 = vector<address> [@0x1, @0x2, @0x3];
        spec {
            assert v1[0] == @0x111 && v1[1] == @0x222 && v1[2] == @0x333;
        };
    }

    public fun array_in_fun_incorrect_bool() {
        let v1 = vector<bool> [false, true, false];
        spec {
            assert v1[0] == false && v1[1] == false && v1[2] == false;
        };
    }
}
