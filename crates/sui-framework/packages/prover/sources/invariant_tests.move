module prover::invariant_tests;

use prover::prover::{requires, ensures, invariant, old};

fun test0_spec(n: u64) {
    let mut i = 0;

    invariant!(|| {
        ensures(i <= n);
    });
    while (i < n) {
        i = i + 1;
    };

    ensures(i == n);
}

fun test1_spec(n: u64): u128 {
    let mut s: u128 = 0;
    let mut i = 0;

    invariant!(|| {
        ensures(i <= n && s == (i as u128) * ((i as u128) + 1) / 2);
    });
    while (i < n) {
        i = i + 1;
        s = s + (i as u128);
    };

    ensures(s == (n as u128) * ((n as u128) + 1) / 2);
    s
}

fun test2_spec(n: u64): u128 {
    let mut s: u128 = 0;
    let mut i = 0;

    invariant!(|| {
        ensures(i <= n);
        ensures(s == (i as u128) * ((i as u128) + 1) / 2);
    });
    while (i < n) {
        i = i + 1;
        s = s + (i as u128);
    };

    ensures(s == (n as u128) * ((n as u128) + 1) / 2);
    s
}

fun test3_spec(mut n: u64): u128 {
    let mut n = n; // TODO: investigate why this is needed
    let mut s: u128 = 0;

    let old_n = old!(&n);
    invariant!(|| {
        ensures(n <= *old_n);
        ensures(s == ((*old_n as u128) - (n as u128)) * ((*old_n as u128) + (n as u128) + 1) / 2);
    });
    while (n > 0) {
        s = s + (n as u128);
        n = n - 1;
    };

    ensures(s == (*old_n as u128) * ((*old_n as u128) + 1) / 2);
    s
}

fun test4_spec(n: u64): u128 {
    requires(0 < n);

    let mut s: u128 = 0;
    let mut i = 0;

    invariant!(|| {
        ensures(i < n);
        ensures(s == (i as u128) * ((i as u128) + 1) / 2);
    });
    loop {
        i = i + 1;
        s = s + (i as u128);
        if (i >= n) {
            break
        }
    };

    ensures(s == (n as u128) * ((n as u128) + 1) / 2);
    s
}

fun test5_spec(n: u64) {
    let mut i = 0;

    while (i < n) {
        i = i + 1;
    };

    ensures(i >= n);
}
