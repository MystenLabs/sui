// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --addresses test=0x0 --simulator

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    {
      "filter": {
        "TransactionKind": "NotSupported"
      }
    }
  ]
}
