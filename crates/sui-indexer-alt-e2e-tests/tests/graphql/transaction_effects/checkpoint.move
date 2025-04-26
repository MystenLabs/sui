// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# programmable --sender A --inputs @A
//> 0: TransferObjects([Gas], Input(0))

//# programmable --sender A --inputs @A
//> 0: TransferObjects([Gas], Input(0))

//# create-checkpoint

//# programmable --sender A --inputs @A
//> 0: TransferObjects([Gas], Input(0))

//# create-checkpoint

//# run-graphql
{ # Fetch the checkpoint for each transaction's effects
  e1: transactionEffects(digest: "@{digest_1}") {
    checkpoint { sequenceNumber }
  }

  e2: transactionEffects(digest: "@{digest_2}") {
    checkpoint { sequenceNumber }
  }

  e4: transactionEffects(digest: "@{digest_4}") {
    checkpoint { sequenceNumber }
  }
}
