// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that --address-balance-gas pays for gas from the address balance,
// leaving owned gas objects untouched.

//# init --addresses test=0x0 --accounts A --enable-address-balance-gas-payments --simulator

// View gas coin before any transactions
//# view-object 0,0

// Empty transaction using address balance gas -- no objects should be modified
//# programmable --sender A --address-balance-gas

// View gas coin after -- balance should be unchanged
//# view-object 0,0

// Send the gas coin to 0x0 while paying with address balance.
// The gas coin balance should not change since gas is paid from address balance.
//# programmable --sender A --inputs @0x0 --address-balance-gas
//> TransferObjects([Gas], Input(0))

//# view-object 0,0
