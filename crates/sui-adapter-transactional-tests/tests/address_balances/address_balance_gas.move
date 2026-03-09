// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that --address-balance-gas pays for gas from the address balance,
// leaving owned gas objects untouched.

//# init --addresses test=0x0 --accounts A --enable-address-balance-gas-payments --simulator

// View gas coin before any transactions
//# view-object 0,0

// Empty transaction using address balance gas
//# programmable --sender A --address-balance-gas

// Use the object, but not as gas
//# programmable --sender A --address-balance-gas --inputs object(0,0)

// View gas coin after -- balance should be unchanged
//# view-object 0,0
