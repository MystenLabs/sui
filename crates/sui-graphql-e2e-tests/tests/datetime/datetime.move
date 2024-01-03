// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --simulator

//# create-checkpoint

// advance the clock by 1ms, next checkpoint timestmap should be 1970-01-01T00:00:00:001Z
//# advance-clock --duration-ns 1000000

//# create-checkpoint

// advance the clock by 1ms, next checkpoint timestmap should be 1970-01-01T00:00:00:002Z
//# advance-clock --duration-ns 1000000

//# create-checkpoint

// advance the clock by 1ms, next checkpoint timestmap should be 1970-01-01T00:00:00:003Z
//# advance-clock --duration-ns 1000000

//# create-checkpoint

// advance the clock by 10ms, next checkpoint timestmap should be 1970-01-01T00:00:00:013Z
//# advance-clock --duration-ns 10000000

//# create-checkpoint

// advance the clock by 2000ms, next checkpoint timestmap should be 1970-01-01T00:00:02:013Z
//# advance-clock --duration-ns 2000000000

//# create-checkpoint

// advance the clock by 990s / 16m30s, next checkpoint timestmap should be 1970-01-01T00:16:32.013Z
//# advance-clock --duration-ns 990000000000

//# create-checkpoint

// advance the clock by 9900s / 2h45m0s, next checkpoint timestmap should be 1970-01-01T03:01:32.013Z
//# advance-clock --duration-ns 9900000000000

//# advance-epoch

//# run-graphql
{
  checkpoint(id:{sequenceNumber: 2}) {
    timestamp
  }
}

//# run-graphql
{
  checkpoint(id:{sequenceNumber: 3}) {
    timestamp
  }
}

//# run-graphql
{
  checkpoint(id:{sequenceNumber: 4}) {
    timestamp
  }
}

//# run-graphql
{
  checkpoint(id:{sequenceNumber: 5}) {
    timestamp
  }
}

//# run-graphql
{
  checkpoint(id:{sequenceNumber: 6}) {
    timestamp
  }
}

//# run-graphql
{
  checkpoint(id:{sequenceNumber: 7}) {
    timestamp
  }
}

//# run-graphql
{
  checkpoint(id:{sequenceNumber: 8}) {
    timestamp
  }
}
