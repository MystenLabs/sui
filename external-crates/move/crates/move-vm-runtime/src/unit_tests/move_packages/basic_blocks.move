module 0x1::a;

public fun loops(mut x: u64, mut y: u64): u64 {
    let a = 10;
    loop {
        if (y < 10) { break };
        while (x < 10) {
            x = x + 1;
        };
        y = y + 1;
    };
    a
}

public enum Maybe<T> has drop {
    Just(T),
    Nothing
}

public fun matcher(mut x: Maybe<u64>, mut y: u64): u64 {
    let a = 10;
    loop {
        if (y < 10) { break };
        match (x) {
            Maybe::Just(n) => { x = Maybe::Just(n + 1) },
            Maybe::Nothing => { x = Maybe::Just(1) },
        };
        y = y + 1;
    };
    a
}
