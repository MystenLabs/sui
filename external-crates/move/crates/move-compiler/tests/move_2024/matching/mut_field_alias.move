module 0x42::m;

public enum E {
    X { x: u64 },
    Y { y: u64 }
}

public fun test() {
    let e = E::Y { y: 1 };
    let value = match (e) {
        E::X { mut x } | E::Y { y: mut x } => {
            x = x + 1;
            x
        }
    };
    assert!(value == 2);
}
