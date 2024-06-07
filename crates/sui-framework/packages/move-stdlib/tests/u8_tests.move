// // Copyright (c) Mysten Labs, Inc.
// // SPDX-License-Identifier: Apache-2.0

// #[test_only]
// module std::u64_tests {
//     const MAX: u64 = 0xFFFF_FFFF_FFFF_FFFFu64;
//     const MAX_PRED: u64 = MAX - 1;

//     const CASES: vector<u64> = vector[
//         0,
//         10,
//         100,
//         1 << 32,
//         1 << 64,
//         MAX_PRED,
//         MAX,
//     ];

//     #[test]
//     fun test_max() {
//         let max = MAX;
//         let mut cases = CASES;
//         while (!cases.is_empty()) {
//             let case = cases.pop_back();
//             assert!(max.max(case) == max);
//             assert!(case.max(max) == max);
//             assert!(max.max(max) == max);
//             assert!(case.max(case) == case);
//             assert!((case.max(1) - 1).max(case) == case);
//             assert!((case.min(MAX_PRED) + 1).max(case) == (case.min(MAX_PRED) + 1));
//         }
//     }

//     #[test]
//     fun test_min() {
//         let max = MAX;
//         let mut cases = CASES;
//         while (!cases.is_empty()) {
//             let case = cases.pop_back();
//             assert!(max.min(case) == case);
//             assert!(case.min(max) == case);
//             assert!(max.min(max) == max);
//             assert!(case.min(case) == case);
//             assert!((case.max(1) - 1).min(case) == case.max(1) - 1);
//             assert!((case.min(MAX_PRED) + 1).min(case) == case);
//         }
//     }
// }
