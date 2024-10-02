// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --simulator --accounts C

// TODO: Short term hack to get around indexer epoch issue
//# create-checkpoint

//# advance-epoch

//# programmable --sender C --inputs 10000000000 @C
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# run 0x3::sui_system::request_add_stake --args object(0x5) object(3,0) @validator_0 --sender C

// TODO: Short term hack to get around indexer epoch issue
//# create-checkpoint

//# advance-epoch

//# run-graphql
{
  objects(filter: {type: "0x3::staking_pool::StakedSui"}) {
    edges {
      node {
        asMoveObject {
          contents {
            type {
              repr
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{
  objects(filter: {type: "0x2"}) {
    edges {
      node {
        asMoveObject {
          contents {
            type {
              repr
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{
  objects(filter: {type: "0x2::coin"}) {
    edges {
      node {
        asMoveObject {
          contents {
            type {
              repr
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{
  objects(filter: {type: "0x2::coin::Coin"}) {
    edges {
      node {
        asMoveObject {
          contents {
            type {
              repr
            }
          }
        }
      }
    }
  }
}

//# run-graphql
# Fetch coins of 0x2::sui::SUI inner type
{
  objects(filter: {type: "0x2::coin::Coin<0x2::sui::SUI>"}) {
    edges {
      node {
        asMoveObject {
          contents {
            type {
              repr
            }
          }
        }
      }
    }
  }
}

//# run-graphql
# Inner type should be fully qualified
{
  objects(filter: {type: "0x2::coin::Coin<ye>"}) {
    edges {
      node {
        asMoveObject {
          contents {
            type {
              repr
            }
          }
        }
      }
    }
  }
}

//# run-graphql
# If caller provides angle brackets, they must be balanced and wrap a valid type
{
  objects(filter: {type: "0x2::coin::Coin<"}) {
    edges {
      node {
        asMoveObject {
          contents {
            type {
              repr
            }
          }
        }
      }
    }
  }
}

//# run-graphql
# Package, module, and name must be valid addresses and identifiers
{
  objects(filter: {type: "0x2::a%::B&"}) {
    edges {
      node {
        asMoveObject {
          contents {
            type {
              repr
            }
          }
        }
      }
    }
  }
}

//# run-graphql
# Empty strings are invalid inputs
{
  objects(filter: {type: "::::"}) {
    edges {
      node {
        asMoveObject {
          contents {
            type {
              repr
            }
          }
        }
      }
    }
  }
}

//# run-graphql
# Primitive types are invalid type filters for objects
{
  objects(filter: {type: "u64"}) {
    edges {
      node {
        asMoveObject {
          contents {
            type {
              repr
            }
          }
        }
      }
    }
  }
}
