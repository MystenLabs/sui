// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Sponsored tx, sponsor's coin pays gas, workload sends the gas coin's
// value to the sponsor's address balance via `send_funds`. The gas-charge
// override redirects the final charge from sponsor's Coin to sponsor's AB.
// Net: B's AB ends with +coin_value - net_gas; A is untouched.

//# init --addresses test=0x0 --accounts A B --enable-address-balance-gas-payments --enable-accumulators

// Sponsored tx: sender = A, sponsor = B. Default gas payment uses B's gas
// coin (object 0,1). Workload `send_funds(Gas, @B)` transfers the gas coin
// to B's AB during execution; `finish_gas_coin` then overrides
// gas-charge-location to AB(B).
//# programmable --sender A --sponsor B --inputs @B
//> sui::coin::send_funds<sui::sui::SUI>(Gas, Input(0))

//# create-checkpoint

// Sponsor B's gas coin should be deleted (consumed by send_funds).
//# view-object 0,1

// B's AB receives a single net Merge = original_coin_value - net_gas.
// A's AB is unchanged (no accumulator object for A exists).
//# view-funds sui::balance::Balance<sui::sui::SUI> B

//# view-funds sui::balance::Balance<sui::sui::SUI> A
