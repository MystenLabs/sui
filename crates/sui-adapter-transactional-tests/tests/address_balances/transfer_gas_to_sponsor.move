// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Sponsored tx, sponsor's coin pays gas, workload
// `TransferObjects([Gas], @B)` transfers the gas coin to the sponsor. The
// override fires (location -> Coin(gas_id)) but is a no-op since the
// location was already Coin. Verifies the gas coin ends up still owned by
// the sponsor (which already owned it) with value reduced by net_gas.

//# init --addresses test=0x0 --accounts A B --enable-address-balance-gas-payments --enable-accumulators

// Sponsored tx: sender = A, sponsor = B. Default gas payment uses B's gas
// coin (0,1). Workload transfers Gas back to B; final gas charge is debited
// from the (still-existing) gas coin's value.
//# programmable --sender A --sponsor B --inputs @B
//> TransferObjects([Gas], Input(0))

//# create-checkpoint

// Gas coin (0,1) should still exist, owned by B, with value reduced by
// net_gas. Storage cost is for the mutation (no new objects created).
//# view-object 0,1
