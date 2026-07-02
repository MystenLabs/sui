// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Inner-loop staleness fixture for the reaching-condition acyclic structurer. The body of
// `inner_loop_two_feed` is a `while` whose body is a two-feed abs_diff staleness cascade —
// the first feed's continuation arm leads into the second feed's check, matching the pyth
// pattern. Reaching-condition structuring should recover the compound condition
// `(now > f1 && now - f1 >= thr) || (now <= f1 && f1 - now >= thr) || …` inside the loop
// body — the proof that reaching fires per-region, not only at the function level.
module staleness::staleness;

public fun feed_ts(feed: u64): u64 {
    feed * 3 + 1
}

// Non-uniform-arm abs_diff diamond — the two stale arms set DIFFERENT state (one sets
// `fresh = false`, the other ALSO bumps a counter). `recognize_diamond`'s body-equivalence
// guard (`bodies_equivalent`) must reject this fold: keeping only s1's body would silently
// drop the counter bump in s2. Expected output: the if-else stays unfolded, NOT collapsed
// into `if (now > t && now - t >= thr || now <= t && t - now >= thr) { fresh = false; }`.
public fun non_uniform_arms(now: u64, t: u64, thr: u64, counter: &mut u64, fresh: &mut bool) {
    if (now > t) {
        if (now - t >= thr) {
            *fresh = false;
        }
    } else {
        if (t - now >= thr) {
            *fresh = false;
            *counter = *counter + 1;
        }
    }
}

public fun inner_loop_two_feed(
    now: u64,
    n: u64,
    thr: u64,
    other: u64,
    fresh_out: &mut bool,
) {
    let mut fresh = true;
    let mut i = 0;
    while (i < n) {
        let f1 = feed_ts(i);
        if (now > f1) {
            if (now - f1 >= thr) {
                fresh = false;
            } else {
                let f2 = feed_ts(other);
                if (now > f2) {
                    if (now - f2 >= thr) {
                        fresh = false;
                    }
                } else {
                    if (f2 - now >= thr) {
                        fresh = false;
                    }
                }
            }
        } else {
            if (f1 - now >= thr) {
                fresh = false;
            } else {
                let f2 = feed_ts(other);
                if (now > f2) {
                    if (now - f2 >= thr) {
                        fresh = false;
                    }
                } else {
                    if (f2 - now >= thr) {
                        fresh = false;
                    }
                }
            }
        };
        i = i + 1;
    };
    *fresh_out = fresh;
}
