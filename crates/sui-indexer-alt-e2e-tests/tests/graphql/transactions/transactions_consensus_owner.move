// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 108 --accounts A B C --simulator

// Create a consensus-owned (party) object owned by B, sent by A
//# programmable --sender A --inputs 1000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::party::single_owner(Input(1));
//> 2: sui::transfer::public_party_transfer<sui::coin::Coin<sui::sui::SUI>>(Result(0), Result(1));

// Create a regular address-owned object sent to C by A
//# programmable --sender A --inputs 2000 @C
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{
  # A sent both transactions
  sentByA: transactions(filter: { sentAddress: "@{A}", atCheckpoint: 1 }) { ...TX }
  # B should only be affected by the party transfer
  affectedB: transactions(filter: { affectedAddress: "@{B}", atCheckpoint: 1 }) { ...TX }
  # C should only be affected by the regular transfer
  affectedC: transactions(filter: { affectedAddress: "@{C}", atCheckpoint: 1 }) { ...TX }
}

fragment TX on TransactionConnection {
  edges {
    cursor
    node {
      digest
    }
  }
}
