// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --simulator

//# run-graphql
{ # Check Checkpoint: 0, it should have network_total_transactions of 1
  c0: checkpoint(sequenceNumber: 0) { networkTotalTransactions }
}

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))


//# create-checkpoint

//# run-graphql
{ # Check Checkpoint: 1, it should have network_total_transactions of 2
  c1: checkpoint(sequenceNumber: 1) { networkTotalTransactions }
}

//# create-checkpoint

//# run-graphql
{ # Check Checkpoint: 2, it should have network_total_transactions of 2
  c2: checkpoint(sequenceNumber: 2) { networkTotalTransactions }
}

//# programmable --sender A --inputs 44 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender B --inputs 43 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{ # Check Checkpoint: 3, it should have network_total_transactions of 4
  c3: checkpoint(sequenceNumber: 3) { networkTotalTransactions }
}

//# run-graphql
{ # Check Checkpoint: 4, non-existent 
  c4: checkpoint(sequenceNumber: 4) { networkTotalTransactions }
}

//# run-graphql
{ # Fetch each checkpoints in a multi-get
  multiGetCheckpoints(keys: [0, 1, 2, 3]) { ...Cp }
}

fragment Cp on Checkpoint {
  sequenceNumber
  networkTotalTransactions
}


//# run-graphql
{ # Multi-get a mix of existing and non-existing checkpoints
  multiGetCheckpoints(keys: [2, 100, 0, 200]) {
    sequenceNumber
    networkTotalTransactions
  }
}