module 0x42::m;

public enum Maybe<T> {
    Just(T),
    Nothing
}


fun helper(_x: u64) { abort 0 }

fun test(x: &Maybe<u64>, y: Maybe<u64>): u64 {
    helper(match (y) { Maybe::Just(n) => n, Maybe::Nothing => 0 });

    let a: u64 = match (x) { Maybe::Just(n) => *n, Maybe::Nothing => 0 };

    let b: u64 = loop {
        break match (x) { Maybe::Just(n) => *n, Maybe::Nothing => 0 }
    };

    let c: u64 = 'a: {
        return 'a match (x) { Maybe::Just(n) => *n, Maybe::Nothing => 0 }
    };

    let d: u64 = 'a: {
        while (true) {
            return 'a match (x) { Maybe::Just(n) => *n, Maybe::Nothing => 0 }
        };
        0
    };

    while (match (x) { Maybe::Just(_) => true, Maybe::Nothing => false }) {
        break
    };

    let e = if (match (x) { Maybe::Just(_) => true, Maybe::Nothing => false, }) return 5 else 0;

    let (f, g) = (
        match (x) { Maybe::Just(n) => *n, Maybe::Nothing => 0 },
        match (x) { Maybe::Just(n) => *n, Maybe::Nothing => 0 }
    );

    let h = match (x) { Maybe::Just(n) => *n, Maybe::Nothing => 0 } + 1;

    let i = 1 + match (x) { Maybe::Just(n) => *n, Maybe::Nothing => 0 };

    let j = match (x) { Maybe::Just(n) => match (x) { Maybe::Just(m) => *n + *m, Maybe::Nothing => 0 }, Maybe::Nothing => 0 };

    let _q = a + b + c + d + e + f + g + h + i + j;

    return match (x) { Maybe::Just(n) => *n, Maybe::Nothing => 0 }
}
