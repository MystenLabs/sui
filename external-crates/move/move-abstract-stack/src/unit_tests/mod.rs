// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::*;

#[test]
fn test_empty_stack() {
    let mut empty = AbstractStack::<usize>::new();
    assert!(empty.is_empty());

    // pop on empty stack
    assert_eq!(empty.pop(), Err(AbsStackError::Underflow));
    assert_eq!(empty.pop_any_n(nonzero(1)), Err(AbsStackError::Underflow));
    assert_eq!(empty.pop_any_n(nonzero(100)), Err(AbsStackError::Underflow));
    assert_eq!(empty.pop_eq_n(nonzero(12)), Err(AbsStackError::Underflow));
    assert_eq!(empty.pop_eq_n(nonzero(112)), Err(AbsStackError::Underflow));

    assert!(empty.is_empty());
}

#[test]
fn test_simple_push_pop() {
    let mut s = AbstractStack::new();
    s.push(1).unwrap();
    assert!(!s.is_empty());
    assert_eq!(s.len(), 1);
    s.assert_run_lengths([1]);
    assert_eq!(s.pop(), Ok(1));
    assert!(s.is_empty());
    assert_eq!(s.len(), 0);

    s.push(1).unwrap();
    s.push(2).unwrap();
    s.push(3).unwrap();
    assert!(!s.is_empty());
    assert_eq!(s.len(), 3);
    s.assert_run_lengths([1, 1, 1]);
    assert_eq!(s.pop(), Ok(3));
    assert_eq!(s.pop(), Ok(2));
    assert_eq!(s.pop(), Ok(1));
    assert!(s.is_empty());
    assert_eq!(s.len(), 0);

    s.push_n(1, 1).unwrap();
    s.push_n(2, 2).unwrap();
    s.push_n(3, 3).unwrap();
    assert!(!s.is_empty());
    assert_eq!(s.len(), 6);
    s.assert_run_lengths([1, 2, 3]);
    assert_eq!(s.pop(), Ok(3));
    assert_eq!(s.pop(), Ok(3));
    assert_eq!(s.pop(), Ok(3));
    assert_eq!(s.pop(), Ok(2));
    assert_eq!(s.pop(), Ok(2));
    assert_eq!(s.pop(), Ok(1));
    assert!(s.is_empty());
    assert_eq!(s.len(), 0);

    s.push_n(1, 1).unwrap();
    s.push_n(2, 2).unwrap();
    s.push_n(3, 3).unwrap();
    assert!(!s.is_empty());
    assert_eq!(s.len(), 6);
    s.assert_run_lengths([1, 2, 3]);
    assert_eq!(s.pop_eq_n(nonzero(3)), Ok(3));
    assert_eq!(s.pop_eq_n(nonzero(2)), Ok(2));
    assert_eq!(s.pop_eq_n(nonzero(1)), Ok(1));
    assert!(s.is_empty());
    assert_eq!(s.len(), 0);

    s.push(1).unwrap();
    s.push(2).unwrap();
    s.push(2).unwrap();
    s.push(3).unwrap();
    s.push(3).unwrap();
    s.push(3).unwrap();
    assert!(!s.is_empty());
    s.assert_run_lengths([1, 2, 3]);
    assert_eq!(s.len(), 6);
    assert_eq!(s.pop_eq_n(nonzero(3)), Ok(3));
    assert_eq!(s.pop_eq_n(nonzero(2)), Ok(2));
    assert_eq!(s.pop_eq_n(nonzero(1)), Ok(1));
    assert!(s.is_empty());
    assert_eq!(s.len(), 0);

    s.push_n(1, 1).unwrap();
    s.push_n(2, 2).unwrap();
    s.push_n(3, 3).unwrap();
    assert!(!s.is_empty());
    s.assert_run_lengths([1, 2, 3]);
    assert_eq!(s.len(), 6);
    s.pop_any_n(nonzero(6)).unwrap();
    assert_eq!(s.len(), 0);
    assert!(s.is_empty());

    s.push(1).unwrap();
    s.push(2).unwrap();
    s.push(2).unwrap();
    s.push(3).unwrap();
    s.push(3).unwrap();
    s.push(3).unwrap();
    assert!(!s.is_empty());
    s.assert_run_lengths([1, 2, 3]);
    assert_eq!(s.len(), 6);
    s.pop_any_n(nonzero(4)).unwrap();
    s.pop_any_n(nonzero(2)).unwrap();
    assert!(s.is_empty());
    assert_eq!(s.len(), 0);
}

#[test]
fn test_not_eq() {
    let mut s = AbstractStack::new();
    s.push(1).unwrap();
    s.push(2).unwrap();
    s.push(2).unwrap();
    s.push(3).unwrap();
    s.push(3).unwrap();
    s.push(3).unwrap();
    assert_eq!(s.len(), 6);
    s.assert_run_lengths([1, 2, 3]);
    assert_eq!(s.pop_eq_n(nonzero(4)), Err(AbsStackError::ElementNotEqual));
    assert_eq!(s.pop_eq_n(nonzero(5)), Err(AbsStackError::ElementNotEqual));
    assert_eq!(s.len(), 6);
    s.assert_run_lengths([1, 2, 3]);
}

#[test]
fn test_not_enough_values() {
    let mut s = AbstractStack::new();
    s.push(1).unwrap();
    s.push(2).unwrap();
    s.push(2).unwrap();
    s.push(3).unwrap();
    s.push(3).unwrap();
    s.push(3).unwrap();
    assert_eq!(s.len(), 6);
    s.assert_run_lengths([1, 2, 3]);
    assert_eq!(s.pop_eq_n(nonzero(7)), Err(AbsStackError::Underflow));
    assert_eq!(s.pop_any_n(nonzero(7)), Err(AbsStackError::Underflow));
    assert_eq!(s.len(), 6);
    s.assert_run_lengths([1, 2, 3]);
}

#[test]
fn test_exhaustive() {
    fn run_lengths(bits: &[bool]) -> Vec<u64> {
        let mut cur = bits[0];
        let mut runs = vec![0];
        for bit in bits.iter().copied() {
            if cur == bit {
                let last = runs.last_mut().unwrap();
                *last += 1;
            } else {
                cur = bit;
                runs.push(1);
            }
        }
        runs
    }
    fn push_pop(bits: &[bool]) {
        let mut s = AbstractStack::new();
        for bit in bits.iter().copied() {
            s.push(bit).unwrap();
        }
        s.assert_run_lengths(run_lengths(bits));

        for bit in bits.iter().copied().rev() {
            assert_eq!(s.pop(), Ok(bit));
        }
        assert!(s.is_empty())
    }
    fn pushn_pop(bits: &[bool]) {
        let mut s = AbstractStack::new();
        let mut push_cur = false;
        let mut push_count = 0;
        for bit in bits.iter().copied() {
            if push_cur == bit {
                push_count += 1;
            } else {
                s.push_n(push_cur, push_count).unwrap();
                push_cur = bit;
                push_count = 1;
            }
        }
        s.push_n(push_cur, push_count).unwrap();
        assert_eq!(s.len(), bits.len() as u64);
        s.assert_run_lengths(run_lengths(bits));

        let mut n = bits.len() as u64;
        for bit in bits.iter().copied().rev() {
            assert_eq!(s.pop(), Ok(bit));
            n -= 1;
            assert_eq!(s.len(), n);
        }
        assert!(s.is_empty())
    }
    fn push_popn(bits: &[bool]) {
        let mut s = AbstractStack::new();
        let mut s_no_eq = AbstractStack::new();
        for bit in bits {
            s.push(*bit).unwrap();
            s_no_eq.push(*bit).unwrap();
        }
        assert_eq!(s.len(), bits.len() as u64);
        assert_eq!(s_no_eq.len(), bits.len() as u64);
        let rl = run_lengths(bits);
        s.assert_run_lengths(&rl);
        s_no_eq.assert_run_lengths(&rl);

        let mut pop_cur = false;
        let mut pop_count = 0;
        let mut n = bits.len() as u64;
        for bit in bits.iter().copied().rev() {
            if pop_cur == bit {
                pop_count += 1
            } else {
                if pop_count > 0 {
                    assert_eq!(s.pop_eq_n(nonzero(pop_count)), Ok(pop_cur));
                    n -= pop_count;
                    assert_eq!(s.len(), n);
                }
                pop_cur = bit;
                pop_count = 1;
            }
        }
        if pop_count > 0 {
            assert_eq!(s.pop_eq_n(nonzero(pop_count)), Ok(pop_cur));
        }
        if !bits.is_empty() {
            s_no_eq.pop_any_n(nonzero(bits.len() as u64)).unwrap();
        }
        assert!(s.is_empty());
        assert!(s_no_eq.is_empty());
    }
    fn pushn_popn(bits: &[bool]) {
        let mut s = AbstractStack::new();
        let mut s_no_eq = AbstractStack::new();
        let mut push_cur = false;
        let mut push_count = 0;
        let mut n = 0;
        for bit in bits.iter().copied() {
            if push_cur == bit {
                push_count += 1;
            } else {
                s.push_n(push_cur, push_count).unwrap();
                s_no_eq.push_n(push_cur, push_count).unwrap();
                n += push_count;
                assert_eq!(s.len(), n);
                assert_eq!(s_no_eq.len(), n);
                push_cur = bit;
                push_count = 1;
            }
        }
        s.push_n(push_cur, push_count).unwrap();
        s_no_eq.push_n(push_cur, push_count).unwrap();
        assert_eq!(s.len(), bits.len() as u64);
        assert_eq!(s_no_eq.len(), bits.len() as u64);
        let rl = run_lengths(bits);
        s.assert_run_lengths(&rl);
        s_no_eq.assert_run_lengths(&rl);

        let mut pop_cur = false;
        let mut pop_count = 0;
        let mut n = bits.len() as u64;
        for bit in bits.iter().copied().rev() {
            if pop_cur == bit {
                pop_count += 1
            } else {
                if pop_count > 0 {
                    assert_eq!(s.pop_eq_n(nonzero(pop_count)), Ok(pop_cur));
                    n -= pop_count;
                    assert_eq!(s.len(), n);
                }
                pop_cur = bit;
                pop_count = 1;
            }
        }
        assert_eq!(s.pop_eq_n(nonzero(pop_count)), Ok(pop_cur));
        s_no_eq.pop_any_n(nonzero(bits.len() as u64)).unwrap();
        assert!(s.is_empty());
        assert!(s_no_eq.is_empty());
    }
    for n in 0u8..=255 {
        let bits: Vec<bool> = (0..=7).map(|i| (n & (1 << i)) != 0).collect();
        push_pop(&bits);
        pushn_pop(&bits);
        push_popn(&bits);
        pushn_popn(&bits);
    }
}

fn nonzero(x: u64) -> NonZeroU64 {
    NonZeroU64::new(x).unwrap()
}
