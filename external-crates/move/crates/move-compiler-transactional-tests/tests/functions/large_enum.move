// tests error after serializing a large enum return value

//# init --edition 2024.alpha

//# publish

module 0x42::m {

public enum X1 {
    Big(u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8),
}

public enum X2 {
    V1(X1, X1, X1),
    V2(X1, X1, X1),
    V3(X1, X1, X1),
}

public enum X3 {
    X2(X2, X2, X2),
    U64(u64),
}

public enum X4 {
    X2(X3, X3, X3),
    U64(u64),
}

entry fun x1(): X1 {
    X1::Big(0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0)
}

entry fun x3(): X3 {
    X3::U64(0)
}

entry fun x4(): X4 {
    X4::U64(0)
}

}

//# run 0x42::m::x1

//# run 0x42::m::x3

//# run 0x42::m::x4
