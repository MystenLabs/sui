//# publish

module 0x42::Test {

    struct Cup<T> { value: T }

    public fun t1<T>(x: T): T {
        x
    }

    public fun t2<T>(x: T): vector<T> {
        vector[x]
    }

    public fun t3<T>(x: T): Cup<T> {
        Cup { value: x }
    }

    public fun t4<T,U>(x: T, y: U): (U, T) {
        (y, x)
    }

}

//# run 0x42::Test::t1 --type-args u64 --args 0

//# run 0x42::Test::t1 --type-args u8 --args 0u8

//# run 0x42::Test::t1 --type-args vector<u8> --args b"wat"

//# run 0x42::Test::t1 --type-args 0x42::Test::Cup<u64> --args 0



//# run 0x42::Test::t2 --type-args u64 --args 0

//# run 0x42::Test::t2 --type-args u8 --args 0u8

//# run 0x42::Test::t2 --type-args vector<u8> --args b"wat"

//# run 0x42::Test::t2 --type-args 0x42::Test::Cup<u64> --args 0



//# run 0x42::Test::t3 --type-args u64 --args 0

//# run 0x42::Test::t3 --type-args u8 --args 0u8

//# run 0x42::Test::t3 --type-args vector<u8> --args b"wat"

//# run 0x42::Test::t3 --type-args 0x42::Test::Cup<u64> --args 0



//# run 0x42::Test::t4 --type-args u64 u8 --args 0 0u8

//# run 0x42::Test::t4 --type-args u8 bool --args 0u8 false

//# run 0x42::Test::t4 --type-args vector<u8> 0x42::Test::Cup<u64> --args b"wat" 0

//# run 0x42::Test::t4 --type-args 0x42::Test::Cup<u64> address --args 0 @0x42
