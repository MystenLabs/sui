// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

// 1. An object owner filter that has some nesting, but is still valid.
// 2. An object owner filter that has too many levels of nesting.
// 3. An object owner filter that has many type filters, but is still valid.
// 3. An object owner filter that has too many type filters.

//# run-jsonrpc
{
  "method": "suix_getOwnedObjects",
  "params": [
    "@{A}",
    {
      "filter": {
        "MatchNone": [{
          "MatchNone": [{
            "Package": "0x2"
          }]
        }]
      }
    }
  ]
}

//# run-jsonrpc
{
  "method": "suix_getOwnedObjects",
  "params": [
    "@{A}",
    {
      "filter": {
        "MatchNone": [{
          "MatchNone": [{
            "MatchNone": [{
              "Package": "0x2"
            }]
          }]
        }]
      }
    }
  ]
}

//# run-jsonrpc
{
  "method": "suix_getOwnedObjects",
  "params": [
    "@{A}",
    {
      "filter": {
        "MatchNone": [
          { "Package": "0x1" },
          { "Package": "0x3" },
          { "Package": "0x4" },
          { "Package": "0x5" },
          { "Package": "0x6" },
          { "Package": "0x7" },
          { "Package": "0x8" },
          { "Package": "0x9" },
          { "Package": "0xa" },
          { "Package": "0xb" }
        ]
      }
    }
  ]
}

//# run-jsonrpc
{
  "method": "suix_getOwnedObjects",
  "params": [
    "@{A}",
    {
      "filter": {
        "MatchNone": [
          { "Package": "0x1" },
          { "Package": "0x3" },
          { "Package": "0x4" },
          { "Package": "0x5" },
          { "Package": "0x6" },
          { "Package": "0x7" },
          { "Package": "0x8" },
          { "Package": "0x9" },
          { "Package": "0xa" },
          { "Package": "0xb" },
          { "Package": "0xc" }
        ]
      }
    }
  ]
}
