// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# run-graphql
{ # epoch w/o checkpoint
  latest: epoch { ...E }

  e0: epoch(epochId: 0) { ...E }
  e1: epoch(epochId: 1) { ...E }
}

fragment E on Epoch {
  totalCheckpoints
}

//# create-checkpoint

//# run-graphql
{ # epoch w/ checkpoint
  latest: epoch { ...E }

  e0: epoch(epochId: 0) { ...E }
  e1: epoch(epochId: 1) { ...E }
}

fragment E on Epoch {
  totalCheckpoints
}

//# advance-epoch

//# run-graphql
{ # new epoch w/o checkpoint
  latest: epoch { ...E }

  e0: epoch(epochId: 0) { ...E }
  e1: epoch(epochId: 1) { ...E }
}

fragment E on Epoch {
  totalCheckpoints
}