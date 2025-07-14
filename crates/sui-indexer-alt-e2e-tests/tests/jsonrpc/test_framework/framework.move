// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --simulator --objects-snapshot-min-checkpoint-lag 2

// This test is checking details about the test runner:
//
// (1) Tests will fail if the JSON-RPC query does not contain methods or
//     params.
//     - No JSON object.
//     - Missing params.
//     - Extra trailing comma (tricky!)
// (2) Displaying response headers is supported.
//
// The test description is at the top because the JSON does not have explicit
// syntax for comments.

//# run-jsonrpc

//# run-jsonrpc
{
  "method": "suix_getReferenceGasPrice"
}

//# run-jsonrpc
{
  "method": "suix_getReferenceGasPrice",
  "params": [],
}

//# run-jsonrpc --show-headers
{
  "method": "suix_getReferenceGasPrice",
  "params": []
}
