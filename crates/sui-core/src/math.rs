// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub fn median(v: &Vec<f64>) -> f64 {
    assert!(!v.is_empty());
    let mut scratch = Vec::with_capacity(v.len());
    scratch.extend(v.iter());
    quicksort(&mut scratch);

    let mid = scratch.len() / 2;
    if scratch.len() % 2 == 1 {
        scratch[mid]
    } else {
        (scratch[mid] + scratch[mid - 1]) / 2.0
    }
}

fn select_pivot(v: &mut [f64]) {
    let idx = rand::random::<usize>() % v.len();
    v.swap(0, idx);
}

fn partition(v: &mut [f64]) -> usize {
    select_pivot(v);
    let pivot = v[0];
    let mut i = 0;
    let mut j = 0;
    let end = v.len() - 1;
    while i < end {
        i += 1;
        if v[i] < pivot {
            v[j] = v[i];
            j += 1;
            v[i] = v[j];
        }
    }
    v[j] = pivot;
    j
}

pub fn quicksort(v: &mut [f64]) {
    if v.len() <= 1 {
        return;
    }
    let pivot = partition(v);
    quicksort(&mut v[..pivot]);
    quicksort(&mut v[(pivot + 1)..]);
}
