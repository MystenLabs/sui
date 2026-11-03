// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Dry-run and dev-inspect must summarize simulated creations without assigning persistent
// fake IDs. Each PTB creates two coins to verify that every simulated output is unnumbered.

//# init --accounts A

//# programmable --sender A --dry-run --inputs 1u64 @A
// Simulated creations are not assigned persistent fake IDs.
//> 0: SplitCoins(Gas, [Input(0), Input(0)]);
//> TransferObjects([NestedResult(0,0), NestedResult(0,1)], Input(1));

//# programmable --sender A --dev-inspect --inputs 1u64 @A
// Dev-inspect follows the same convention.
//> 0: SplitCoins(Gas, [Input(0), Input(0)]);
//> TransferObjects([NestedResult(0,0), NestedResult(0,1)], Input(1));
