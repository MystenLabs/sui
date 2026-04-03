module 0x1::extended;

public fun conditional_blocks(x: u64): u64 {
    if (x > 5) {
        if (x > 10) {
            x * 2
        } else {
            x + 1
        }
    } else {
        x - 1
    }
}

public fun nested_loops(mut x: u64, y: u64): u64 {
    while (x < 100) {
        let mut z = 0;
        while (z < y) {
            z = z + 1;
        };
        x = x + z;
    };
    x
}

public fun early_return(x: u64): u64 {
    if (x == 0) {
        return 42
    };
    if (x == 1) {
        return 100
    };
    x * x
}

public fun complex_match(value: u64): u64 {
    let result = if (value < 10) {
        match (value) {
            0 => 1,
            1 => 2,
            2 => 4,
            _ => value * 2,
        }
    } else if (value < 100) {
        value / 2
    } else {
        value / 4
    };
    result
}

public fun loop_with_break_continue(mut x: u64): u64 {
    let mut sum = 0;
    loop {
        if (x == 0) {
            break
        };
        if (x % 2 == 0) {
            x = x - 1;
            continue
        };
        sum = sum + x;
        x = x - 1;
    };
    sum
}

public fun dead_code_example(x: u64): u64 {
    let result = x + 1;
    let _unused = 42; // This should be optimized away
    let _another_unused = x * 2; // This should be optimized away
    result
}

public enum Status {
    Active,
    Inactive,
    Pending,
}

public fun enum_matching(status: Status, value: u64): u64 {
    match (status) {
        Status::Active => {
            if (value > 0) {
                value * 2
            } else {
                1
            }
        },
        Status::Inactive => 0,
        Status::Pending => {
            let mut temp = value;
            while (temp > 10) {
                temp = temp / 2;
            };
            temp
        },
    }
}
