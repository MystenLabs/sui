// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests various invalid usages of a borrowed arg in non-Move calls

//# init --addresses test=0x0 --accounts A B

//# programmable --sender A
//> MergeCoins(Gas, [Gas])

//# programmable --sender B --inputs @A
//> TransferObjects([Gas], Input(0))

//# programmable --sender A --inputs object(0,1)
//> MergeCoins(Input(0), [Input(0)])

//# programmable --sender A --inputs object(0,1)
//> MakeMoveVec([Input(0), Input(0), Input(0)])
