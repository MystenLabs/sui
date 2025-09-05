// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# create-checkpoint

//# advance-epoch

//# create-checkpoint

//# create-checkpoint

//# advance-epoch

//# create-checkpoint

//# run-graphql
{
  c0: checkpoint(sequenceNumber: 0) { epoch { epochId } }
  c1: checkpoint(sequenceNumber: 1) { epoch { epochId } }
  c2: checkpoint(sequenceNumber: 2) { epoch { epochId } }
  c3: checkpoint(sequenceNumber: 3) { epoch { epochId } }
  c4: checkpoint(sequenceNumber: 4) { epoch { epochId } }
  c5: checkpoint(sequenceNumber: 5) { epoch { epochId } }
  c6: checkpoint(sequenceNumber: 6) { epoch { epochId } }
  c7: checkpoint(sequenceNumber: 7) { epoch { epochId } }
}
