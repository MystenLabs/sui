// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# advance-epoch

//# create-checkpoint

//# programmable --sender A --inputs object(1,0) 1
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: MergeCoins(Gas, [Result(0)])

//# programmable --sender A --inputs object(1,0) 2
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: MergeCoins(Gas, [Result(0)])

//# advance-clock --duration-ns 321000000

//# create-checkpoint

//# run-graphql
{ # Fetch each checkpoint individually, and then in a multi-get
  c0: checkpoint(sequenceNumber: 0) { ...Cp }
  c1: checkpoint(sequenceNumber: 1) { ...Cp }
  c2: checkpoint(sequenceNumber: 2) { ...Cp }
  multiGetCheckpoints(keys: [0, 1, 2]) { ...Cp }
}

fragment Cp on Checkpoint {
  sequenceNumber
  validatorSignatures {
    epoch { epochId }
    signature
    signersMap
  }
}

//# run-graphql
{ # Fetch a non-existent checkpoint
  checkpoint(sequenceNumber: 4) {
    sequenceNumber
    validatorSignatures {
      epoch { epochId }
      signature
      signersMap
    }
  }
}

//# run-graphql
{ # Multi-get a mix of existing and non-existing checkpoints
  multiGetCheckpoints(keys: [2, 100, 0, 200]) {
    sequenceNumber
    validatorSignatures {
      epoch { epochId }
      signature
      signersMap
    }
  }
}