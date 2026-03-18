// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests transferring the GasCoin by value when using --address-balance-gas.

//# init --addresses test=0x0 --accounts A --enable-address-balance-gas-payments --enable-accumulators

// First send funds to A's address balance so we can pay for gas from it
//# programmable --sender A --inputs 10000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

// Transfer gas coin to 0x0 via TransferObjects while paying with address balance
//# programmable --sender A --inputs @0x0 object(0,0) --address-balance-gas
//> TransferObjects([Gas], Input(0))

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A
