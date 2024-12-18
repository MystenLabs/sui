module 0x42::m;

public enum Option<T> has drop {
    None,
    Some(T)
}

fun t1(): u64 {
    let _value = false;
    let o: Option<u64> = Option::None;
    match (o) {
        Option::None  => 0u64,
        Option::Some(val) if (*val < 100 && val != 42) => val,
        Option::None => 2,
        Option::Some(_) => 3,
    }
}

fun t2(): u64 {
    let _value = false;
    let o: Option<u64> = Option::None;
    match (o) {
        _ if (_value) => 0u64,
        Option::None  => 0u64,
        Option::Some(val) if (_value) => val,
        Option::None => 2,
        Option::Some(_) => 3,
    }
}

fun t3(): u64 {
    let _value = false;
    let o: Option<u64> = Option::None;
    match (o) {
        _  => 0u64,
        _  => 0u64,
        Option::None  => 0u64,
        _ if (true) => 0u64,
        Option::None => 2,
        Option::Some(_) => 3,
    }
}

fun t4(): u64 {
    let _value = false;
    let o: Option<u64> = Option::None;
    match (o) {
        _  => 0u64,
        Option::Some(val) if (val == 0) => val,
        Option::None  => 0u64,
        Option::None  => 0u64,
        Option::Some(val)  => val,
        Option::None => 2,
        Option::Some(_) => 3,
    }
}

fun t5(): u64 {
    let _value = false;
    let o: Option<u64> = Option::None;
    match (o) {
        Option::None  => 0u64,
        Option::Some(val)  => val,
        _  => 0u64,
        Option::None  => 0u64,
        _  => 0u64,
        _ if (_value) => 0u64,
        Option::None => 2,
        Option::Some(_) => 3,
    }
}

fun t6(): u64 {
    let _value = false;
    let o: Option<u64> = Option::None;
    match (o) {
        Option::Some(val)  => val,
        Option::None  => 0u64,
        Option::Some(val) if (_value) => val,
        Option::Some(val)  => val,
        Option::Some(val) if (*val < 100 && val != 42) => val,
        _ if (_value) => 0u64,
        _ if (_value) => 0u64,
        Option::None => 2,
        Option::Some(_) => 3,
    }
}

fun t7(): u64 {
    let _value = false;
    let o: Option<u64> = Option::None;
    match (o) {
        _  => 0u64,
        _  => 0u64,
        Option::None  => 0u64,
        Option::Some(val)  => val,
        Option::None  => 0u64,
        _ if (_value) => 0u64,
        _  => 0u64,
        _ if (_value) => 0u64,
        Option::None => 2,
        Option::Some(_) => 3,
    }
}

fun t8(): u64 {
    let _value = false;
    let o: Option<u64> = Option::None;
    match (o) {
        Option::Some(val) if (true) => val,
        Option::Some(val) if (*val < 100 && val != 42) => val,
        Option::None  => 0u64,
        Option::Some(val) if (*val > 50) => val,
        Option::Some(val) if (val == 0) => val,
        Option::Some(val) if (_value) => val,
        _ if (true) => 0u64,
        Option::None  => 0u64,
        Option::Some(val) if (*val < 100 && val != 42) => val,
        Option::None => 2,
        Option::Some(_) => 3,
    }
}

fun t9(): u64 {
    let _value = false;
    let o: Option<u64> = Option::None;
    match (o) {
        Option::Some(val) if (*val < 100 && val != 42) => val,
        _ if (_value) => 0u64,
        Option::None  => 0u64,
        Option::None  => 0u64,
        Option::Some(val)  => val,
        Option::None  => 0u64,
        Option::None  => 0u64,
        _ if (_value) => 0u64,
        Option::None  => 0u64,
        _ if (true) => 0u64,
        Option::None => 2,
        Option::Some(_) => 3,
    }
}

fun t10(): u64 {
    let _value = false;
    let o: Option<u64> = Option::None;
    match (o) {
        Option::Some(val) if (*val < 100 && val != 42) => val,
        Option::Some(val)  => val,
        Option::None  => 0u64,
        _ if (true) => 0u64,
        _  => 0u64,
        Option::Some(val)  => val,
        Option::None  => 0u64,
        _  => 0u64,
        Option::Some(val)  => val,
        Option::None  => 0u64,
        _  => 0u64,
        Option::None => 2,
        Option::Some(_) => 3,
    }
}

fun t11(): u64 {
    let _value = false;
    let o: Option<u64> = Option::None;
    match (o) {
        Option::Some(val) if (_value) => val,
        _ if (true) => 0u64,
        _  => 0u64,
        Option::None  => 0u64,
        _ if (_value) => 0u64,
        Option::None  => 0u64,
        _  => 0u64,
        Option::None  => 0u64,
        _ if (_value) => 0u64,
        Option::None  => 0u64,
        _  => 0u64,
        Option::None  => 0u64,
        Option::None => 2,
        Option::Some(_) => 3,
    }
}

fun t12(): u64 {
    let _value = false;
    let o: Option<u64> = Option::None;
    match (o) {
        Option::None  => 0u64,
        _  => 0u64,
        _  => 0u64,
        Option::None  => 0u64,
        Option::None  => 0u64,
        Option::None  => 0u64,
        _  => 0u64,
        Option::Some(val) if (*val > 50) => val,
        Option::Some(val) if (*val > 50) => val,
        Option::None  => 0u64,
        _  => 0u64,
        Option::Some(val)  => val,
        _  => 0u64,
        Option::None => 2,
        Option::Some(_) => 3,
    }
}

fun t13(): u64 {
    let _value = false;
    let o: Option<u64> = Option::None;
    match (o) {
        Option::None  => 0u64,
        Option::None  => 0u64,
        _ if (_value) => 0u64,
        Option::Some(val)  => val,
        _  => 0u64,
        _ if (true) => 0u64,
        _  => 0u64,
        Option::None  => 0u64,
        Option::Some(val)  => val,
        Option::None  => 0u64,
        Option::Some(val)  => val,
        _  => 0u64,
        _  => 0u64,
        Option::None  => 0u64,
        Option::None => 2,
        Option::Some(_) => 3,
    }
}

fun t14(): u64 {
    let _value = false;
    let o: Option<u64> = Option::None;
    match (o) {
        Option::Some(val) if (val == 0) => val,
        Option::Some(val) if (*val > 50) => val,
        Option::None  => 0u64,
        Option::Some(val)  => val,
        _  => 0u64,
        Option::Some(val) if (*val > 50) => val,
        _  => 0u64,
        Option::None  => 0u64,
        Option::Some(val)  => val,
        _ if (true) => 0u64,
        _  => 0u64,
        Option::Some(val) if (val == 0) => val,
        Option::Some(val) if (*val > 50) => val,
        Option::Some(val) if (*val > 50) => val,
        _  => 0u64,
        Option::None => 2,
        Option::Some(_) => 3,
    }
}

fun t15(): u64 {
    let _value = false;
    let o: Option<u64> = Option::None;
    match (o) {
        Option::None  => 0u64,
        _ if (_value) => 0u64,
        _ if (_value) => 0u64,
        Option::None  => 0u64,
        _  => 0u64,
        Option::None  => 0u64,
        _  => 0u64,
        Option::Some(val) if (*val < 100 && val != 42) => val,
        Option::None  => 0u64,
        Option::None  => 0u64,
        Option::None  => 0u64,
        Option::None  => 0u64,
        Option::Some(val)  => val,
        Option::None  => 0u64,
        Option::Some(val) if (*val > 50) => val,
        Option::None  => 0u64,
        Option::None => 2,
        Option::Some(_) => 3,
    }
}

fun t16(): u64 {
    let _value = false;
    let o: Option<u64> = Option::None;
    match (o) {
        Option::Some(val) if (val == 0) => val,
        _  => 0u64,
        _ if (_value) => 0u64,
        _ if (true) => 0u64,
        Option::None  => 0u64,
        Option::Some(val) if (*val < 100 && val != 42) => val,
        _  => 0u64,
        _ if (_value) => 0u64,
        Option::Some(val) if (val == 0) => val,
        Option::Some(val) if (_value) => val,
        _  => 0u64,
        _  => 0u64,
        _  => 0u64,
        Option::Some(val)  => val,
        _  => 0u64,
        Option::None  => 0u64,
        Option::None  => 0u64,
        Option::None => 2,
        Option::Some(_) => 3,
    }
}

fun t17(): u64 {
    let _value = false;
    let o: Option<u64> = Option::None;
    match (o) {
        Option::None  => 0u64,
        Option::Some(val) if (_value) => val,
        _ if (true) => 0u64,
        Option::Some(val) if (*val < 100 && val != 42) => val,
        _  => 0u64,
        Option::Some(val) if (_value) => val,
        Option::None  => 0u64,
        Option::None  => 0u64,
        Option::None  => 0u64,
        _  => 0u64,
        _  => 0u64,
        Option::None  => 0u64,
        Option::Some(val) if (*val < 100 && val != 42) => val,
        _  => 0u64,
        Option::Some(val) if (*val < 100 && val != 42) => val,
        Option::Some(val) if (_value) => val,
        Option::Some(val) if (*val > 50) => val,
        Option::None  => 0u64,
        Option::None => 2,
        Option::Some(_) => 3,
    }
}

fun t18(): u64 {
    let _value = false;
    let o: Option<u64> = Option::None;
    match (o) {
        Option::Some(val)  => val,
        Option::None  => 0u64,
        Option::Some(val)  => val,
        Option::None  => 0u64,
        Option::Some(val) if (*val > 50) => val,
        Option::Some(val) if (val == 0) => val,
        Option::Some(val)  => val,
        Option::None  => 0u64,
        _  => 0u64,
        Option::Some(val) if (true) => val,
        _  => 0u64,
        _  => 0u64,
        Option::None  => 0u64,
        _ if (_value) => 0u64,
        Option::None  => 0u64,
        Option::Some(val)  => val,
        Option::None  => 0u64,
        Option::Some(val)  => val,
        Option::Some(val)  => val,
        Option::None => 2,
        Option::Some(_) => 3,
    }
}

fun t19(): u64 {
    let _value = false;
    let o: Option<u64> = Option::None;
    match (o) {
        Option::Some(val)  => val,
        Option::None  => 0u64,
        _ if (true) => 0u64,
        Option::Some(val)  => val,
        Option::Some(val) if (*val < 100 && val != 42) => val,
        _  => 0u64,
        Option::Some(val) if (val == 0) => val,
        Option::Some(val) if (*val > 50) => val,
        Option::Some(val) if (_value) => val,
        Option::None  => 0u64,
        Option::None  => 0u64,
        Option::Some(val)  => val,
        Option::None  => 0u64,
        _  => 0u64,
        Option::Some(val) if (*val < 100 && val != 42) => val,
        _ if (true) => 0u64,
        Option::None  => 0u64,
        _  => 0u64,
        Option::Some(val) if (val == 0) => val,
        Option::None  => 0u64,
        Option::None => 2,
        Option::Some(_) => 3,
    }
}

fun t20(): u64 {
    let _value = false;
    let o: Option<u64> = Option::None;
    match (o) {
        _ if (true) => 0u64,
        _ if (_value) => 0u64,
        Option::Some(val)  => val,
        Option::Some(val) if (val == 0) => val,
        Option::None  => 0u64,
        _ if (_value) => 0u64,
        _  => 0u64,
        Option::Some(val)  => val,
        Option::Some(val) if (_value) => val,
        Option::None  => 0u64,
        Option::Some(val) if (*val < 100 && val != 42) => val,
        Option::Some(val) if (*val < 100 && val != 42) => val,
        Option::Some(val)  => val,
        _ if (true) => 0u64,
        Option::Some(val) if (val == 0) => val,
        _  => 0u64,
        Option::None  => 0u64,
        Option::None  => 0u64,
        Option::Some(val) if (_value) => val,
        _  => 0u64,
        Option::None  => 0u64,
        Option::None => 2,
        Option::Some(_) => 3,
    }
}


