// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Per-transaction `max_withdraws = 10` cap. The cap counts explicit
// WithdrawFunds PTB commands plus coin reservations, with +1 for the
// implicit gas-budget withdrawal only when gas is paid purely from address
// balance (i.e. no explicit gas-payment entries). When all gas payments are
// explicit `--gas-payment withdraw(...)`, the payment list is non-empty, so
// the implicit +1 is not added; the cap applies directly to the count of
// withdraws.
//   - 10 explicit reservations: at boundary, passes.
//   - 11 explicit reservations: over limit, rejects.

//# init --addresses test=0x0 --accounts A --enable-address-balance-gas-payments --enable-coin-reservations --enable-accumulators

// Seed A's address balance with plenty.
//# programmable --sender A --inputs 1000000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# create-checkpoint

// 10 withdraw reservations, each 20M. Total available 200M, budget 100M, plenty for workload. Boundary case, should PASS.
//# programmable --sender A --gas-budget 100000000 --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(20000000) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(20000000) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(20000000) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(20000000) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(20000000) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(20000000) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(20000000) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(20000000) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(20000000) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(20000000) --inputs 100 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

// 11 withdraw reservations, each 20M. Total available 220M. Over limit, should REJECT at validation.
//# programmable --sender A --gas-budget 100000000 --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(20000000) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(20000000) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(20000000) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(20000000) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(20000000) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(20000000) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(20000000) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(20000000) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(20000000) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(20000000) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(20000000) --inputs 100 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))
