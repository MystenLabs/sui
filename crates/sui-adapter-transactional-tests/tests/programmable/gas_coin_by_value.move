// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests valid gas coin usage by value

//# init --addresses test=0x0 --accounts A B --enable-accumulators --enable-address-balance-gas-payments

//# programmable --sender A --inputs @B
//> TransferObjects([Gas], Input(0))

//# view-object 0,0

//# programmable --sender B --inputs @A --gas-payment object(0,0)
//> sui::coin::send_funds<sui::sui::SUI>(Gas, Input(0))

//# view-object 0,0

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

//# view-funds sui::balance::Balance<sui::sui::SUI> B
