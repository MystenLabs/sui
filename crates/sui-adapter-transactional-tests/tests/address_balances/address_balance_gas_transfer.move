// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests transferring the GasCoin by value when using --address-balance-gas.

//# init --addresses test=0x0 --accounts A B --enable-address-balance-gas-payments --enable-accumulators

//# programmable --sender A --inputs 10000000000 @A
// First send funds to A's address balance so we can pay for gas from it
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender B --inputs 10000000000 @B
// First send funds to B's address balance so we can pay for gas from it
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

//# programmable --sender A --inputs @0x0 object(0,0) --address-balance-gas
// Transfer gas coin to 0x0 via TransferObjects while paying with address balance
//> TransferObjects([Gas], Input(0))

//# programmable --sender B --inputs @B object(0,1) --address-balance-gas
// Transfer gas coin to the sender to show no object was created
//> TransferObjects([Gas], Input(0))
