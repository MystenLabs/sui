// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# advance-epoch

//# programmable --sender A --inputs object(1,0) 1
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: MergeCoins(Gas, [Result(0)])

//# programmable --sender A --inputs object(1,0) 2
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# advance-epoch

//# advance-clock --duration-ns 321000000

//# create-checkpoint

//# run-graphql
{ # Fetch each checkpoint individually, and then in a multi-get
  c0: checkpoint(sequenceNumber: 0) { ...Cp }
  c1: checkpoint(sequenceNumber: 1) { ...Cp }
  c2: checkpoint(sequenceNumber: 2) { ...Cp }
  c3: checkpoint(sequenceNumber: 3) { ...Cp }
  c4: checkpoint(sequenceNumber: 4) { ...Cp }
  c5: checkpoint(sequenceNumber: 5) { ...Cp }
  # Fetch a non-existent checkpoint c6
  c6: checkpoint(sequenceNumber: 6) { ...Cp }
  # Multi-get a mix of existing and non-existing checkpoints
  multiGetCheckpoints(keys: [0, 1, 2, 3, 4, 5, 6]) { ...Cp }
}

fragment Cp on Checkpoint {
  sequenceNumber
  epoch { epochId }
  rollingGasSummary { 
    computationCost,
    storageCost, 
    storageRebate, 
    nonRefundableStorageFee
  }
}

//# run-graphql
{ # Fetch partial fields on rollingGasSummary with non-zero values
  c4: checkpoint(sequenceNumber: 4) { ...Cp }
  c5: checkpoint(sequenceNumber: 5) { ...Cp }
}

fragment Cp on Checkpoint {
  sequenceNumber
  epoch { epochId }
  rollingGasSummary { 
    computationCost,
    storageCost
  }
}


//# run-graphql
{ # Multi-get a mix of existing and non-existing checkpoints and partial fields
  multiGetCheckpoints(keys: [2, 100, 0, 5, 200]) {
    sequenceNumber
    epoch { epochId }
    rollingGasSummary { 
      computationCost,
      storageCost, 
    }
  }
}