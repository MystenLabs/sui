// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P0=0x0 P1=0x0 --simulator

// 1. Pick transactions based on the package they have called into
// 2. ...the module...
// 3. ...the function...
// 4. Try and pass a bad query (function but no module)
// 5. Try and pass a bad query (No package)

//# publish
module P0::M {
  public struct P0MFoo() has copy, drop, store;

  public fun foo() { sui::event::emit(P0MFoo()) }
}

module P0::N {
  public struct P0NBar() has copy, drop, store;
  public struct P0NBaz() has copy, drop, store;

  public fun bar() { sui::event::emit(P0NBar()) }
  public fun baz() { sui::event::emit(P0NBaz()) }
}

//# publish
module P1::M {
  public struct P1MQux() has copy, drop, store;

  public fun qux() { sui::event::emit(P1MQux()) }
}

//# programmable
//> P0::M::foo();

//# programmable
//> P0::N::bar();

//# programmable
//> P0::N::baz();

//# programmable
//> P1::M::qux();

//# programmable
//> P0::M::foo();

//# programmable
//> P0::N::bar();

//# programmable
//> P0::N::baz();

//# programmable
//> P1::M::qux();

//# programmable
//> P0::M::foo();

//# programmable
//> P0::N::bar();

//# programmable
//> P0::N::baz();

//# programmable
//> P1::M::qux();

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    {
      "filter": {
        "MoveFunction": {
          "package": "@{P0}"
        }
      },
      "options": {
        "showEvents": true
      }
    }
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    {
      "filter": {
        "MoveFunction": {
          "package": "@{P0}",
          "module": "N"
        }
      },
      "options": {
        "showEvents": true
      }
    }
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    {
      "filter": {
        "MoveFunction": {
          "package": "@{P1}",
          "module": "M",
          "function": "qux"
        }
      },
      "options": {
        "showEvents": true
      }
    }
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    {
      "filter": {
        "MoveFunction": {
          "package": "@{P0}",
          "function": "bar"
        }
      },
      "options": {
        "showEvents": true
      }
    }
  ]
}

//# run-jsonrpc
{
  "method": "suix_queryTransactionBlocks",
  "params": [
    {
      "filter": {
        "MoveFunction": {
          "module": "M"
        }
      },
      "options": {
        "showEvents": true
      }
    }
  ]
}
