// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests valid gas coin usage by value

//# init --addresses test=0x0 --accounts A B

//# programmable --sender A --inputs @B
//> TransferObjects([Gas], Input(0))

//# view-object 0,0
