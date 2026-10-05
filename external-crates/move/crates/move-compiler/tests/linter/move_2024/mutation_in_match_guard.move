module 0x42::m;

fun bump(value: &mut u64): bool {
    *value = *value + 1;
    false
}

fun direct_mutation(): u64 {
    let mut value = 0u64;
    let _result = match (true) {
        _ if ({ value = value + 1; false }) => 0u64,
        _ => 0u64,
    };
    value
}

fun mutable_call(): u64 {
    let mut value = 0u64;
    let _result = match (true) {
        _ if (bump(&mut value)) => 0u64,
        _ => 0u64,
    };
    value
}

fun mutation_through_reference(): u64 {
    let mut value = 0u64;
    let reference = &mut value;
    let _result = match (true) {
        _ if ({ *reference = *reference + 1; false }) => 0u64,
        _ => 0u64,
    };
    value
}

fun is_zero(value: &u64): bool {
    *value == 0
}

fun immutable_call(): u64 {
    let value = 0u64;
    match (true) {
        _ if (is_zero(&value)) => 0u64,
        _ => 0u64,
    }
}

fun unused_mutable_borrow(): u64 {
    let mut value = 0u64;
    let _result = match (true) {
        _ if ({ let _borrow = &mut value; false }) => 0u64,
        _ => 0u64,
    };
    value
}

#[allow(lint(mutation_in_match_guard))]
fun allowed_mutation(): u64 {
    let mut value = 0u64;
    let _result = match (true) {
        _ if ({ value = value + 1; false }) => 0u64,
        _ => 0u64,
    };
    value
}
