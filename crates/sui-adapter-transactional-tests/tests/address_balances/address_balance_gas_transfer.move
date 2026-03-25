// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests using the GasCoin when using --address-balance-gas.

//# init --addresses test=0x0 --accounts A B C  --enable-address-balance-gas-payments --enable-accumulators

//# programmable --sender A --inputs 10000000000 @A
// First send funds to A's address balance so we can pay for gas from it
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender B --inputs 10000000000 @B
// First send funds to B's address balance so we can pay for gas from it
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender C --inputs 10000000000 @C
// First send funds to C's address balance so we can pay for gas from it
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

//# view-funds sui::balance::Balance<sui::sui::SUI> B

//# view-funds sui::balance::Balance<sui::sui::SUI> C

//# programmable --sender A --inputs @0x0 object(0,0) --address-balance-gas
// Transfer gas coin to 0x0 via TransferObjects while paying with address balance
//> TransferObjects([Gas], Input(0))

//# view-object 8,0

//# programmable --sender B --inputs @B object(0,1) --address-balance-gas
// Transfer gas coin to the sender to show an object was created, even at the same address
//> TransferObjects([Gas], Input(0))

//# view-object 10,0

//# programmable --sender C --inputs @C 0 object(0,2) --address-balance-gas
// Split off from the gas coin and transfer that to self. Only one coin in total should be created
//> 0: SplitCoins(Gas, [Input(1)]);
//> 1: TransferObjects([Result(0)], Input(0))

//# view-object 12,0

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A

//# view-funds sui::balance::Balance<sui::sui::SUI> B

//# view-funds sui::balance::Balance<sui::sui::SUI> C
