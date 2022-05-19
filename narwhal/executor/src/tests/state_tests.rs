// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn next_transaction() {
    let mut state = ExecutionIndices::default();
    state.next(
        /* total_batches */ 10, /* total_transactions */ 100,
    );
    assert_eq!(state.next_certificate_index, 0);
    assert_eq!(state.next_batch_index, 0);
    assert_eq!(state.next_transaction_index, 1);
}

#[test]
fn next_batch() {
    let mut state = ExecutionIndices::default();
    state.next(/* total_batches */ 10, /* total_transactions */ 1);
    assert_eq!(state.next_certificate_index, 0);
    assert_eq!(state.next_batch_index, 1);
    assert_eq!(state.next_transaction_index, 0);
}

#[test]
fn next_certificate() {
    let mut state = ExecutionIndices::default();
    state.next(/* total_batches */ 1, /* total_transactions */ 1);
    assert_eq!(state.next_certificate_index, 1);
    assert_eq!(state.next_batch_index, 0);
    assert_eq!(state.next_transaction_index, 0);
}

#[test]
fn skip_batch() {
    let mut state = ExecutionIndices {
        next_certificate_index: 1,
        next_batch_index: 1,
        next_transaction_index: 1,
    };
    state.skip_batch(/* total_batches */ 10);
    assert_eq!(state.next_certificate_index, 1);
    assert_eq!(state.next_batch_index, 2);
    assert_eq!(state.next_transaction_index, 0);
}

#[test]
fn skip_certificate() {
    let mut state = ExecutionIndices {
        next_certificate_index: 1,
        next_batch_index: 1,
        next_transaction_index: 1,
    };
    state.skip_certificate();
    assert_eq!(state.next_certificate_index, 2);
    assert_eq!(state.next_batch_index, 0);
    assert_eq!(state.next_transaction_index, 0);
}

#[test]
fn order_certificate_index() {
    let state_1 = ExecutionIndices {
        next_certificate_index: 0,
        next_batch_index: 1,
        next_transaction_index: 1,
    };
    let state_2 = ExecutionIndices {
        next_certificate_index: 1,
        next_batch_index: 0,
        next_transaction_index: 0,
    };

    assert!(state_2 > state_1);
}

#[test]
fn order_batch_index() {
    let state_1 = ExecutionIndices {
        next_certificate_index: 1,
        next_batch_index: 0,
        next_transaction_index: 1,
    };
    let state_2 = ExecutionIndices {
        next_certificate_index: 1,
        next_batch_index: 1,
        next_transaction_index: 0,
    };

    assert!(state_2 > state_1);
}

#[test]
fn order_transaction_index() {
    let state_1 = ExecutionIndices {
        next_certificate_index: 1,
        next_batch_index: 1,
        next_transaction_index: 0,
    };
    let state_2 = ExecutionIndices {
        next_certificate_index: 1,
        next_batch_index: 1,
        next_transaction_index: 1,
    };

    assert!(state_2 > state_1);
}
