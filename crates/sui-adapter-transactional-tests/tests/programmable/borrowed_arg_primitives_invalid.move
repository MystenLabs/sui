// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests various invalid usages of a borrowed arg in non-Move calls

//# init --addresses test=0x0 --accounts A

//# programmable --sender A
//> MergeCoins(Gas, [Gas])

//# programmable --sender A --inputs object(103)
//> MergeCoins(Input(0), [Input(0)])

//# programmable --sender A --inputs object(103)
//> MakeMoveVec([Input(0), Input(0), Input(0)])
