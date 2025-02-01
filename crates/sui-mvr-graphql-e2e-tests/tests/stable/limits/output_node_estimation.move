// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses A=0x42 --simulator

//# run-graphql --show-usage
# pageInfo does not inherit connection's weights
{
  transactionBlocks(first: 20) {                            # 1
    pageInfo {                                              # 1
      hasPreviousPage                                       # 1
    }
  }
}

//# run-graphql --show-usage
# if connection does not have 'first' or 'last' set, use default_page_size (20)
{
  transactionBlocks {                                       # 1
    edges {                                                 # 1
      node {                                                # 20
        digest                                              # 20
      }
    }
  }
}

//# run-graphql --show-usage
# build on previous example with nested connection
{
  checkpoints {                                             # 1
    nodes {                                                 # 1
      transactionBlocks {                                   # 20
        edges {                                             # 20
          txns: node {                                      # 400
            digest                                          # 400
          }
        }
      }
    }
  }
}

//# run-graphql --show-usage
# handles 1
{
  checkpoints {                                             # 1
    nodes {                                                 # 1
      notOne: transactionBlocks {                           # 20
        edges {                                             # 20
          txns: node {                                      # 400
            digest                                          # 400
          }
        }
      }
      isOne: transactionBlocks(first: 1) {                  # 20
        edges {                                             # 20
          txns: node {                                      # 20
            digest                                          # 20
          }
        }
      }
    }
  }
}

//# run-graphql --show-usage
# handles 0
{
  checkpoints {                                             # 1
    nodes {                                                 # 1
      notZero: transactionBlocks {                          # 20
        edges {                                             # 20
          txns: node {                                      # 400
            digest                                          # 400
          }
        }
      }
      isZero: transactionBlocks(first: 0) {                 # 20
        edges {                                             # 20
          txns: node {                                      # 0
            digest                                          # 0
          }
        }
      }
    }
  }
}

//# run-graphql --show-usage
# if connection does have 'first' set, use it
{
  transactionBlocks(first: 1) {                             # 1
    edges {                                                 # 1
      txns: node {                                          # 1
        digest                                              # 1
      }
    }
  }
}

//# run-graphql --show-usage
# if connection does have 'last' set, use it
{
  transactionBlocks(last: 1) {                              # 1
    edges {                                                 # 1
      txns: node {                                          # 1
        digest                                              # 1
      }
    }
  }
}

//# run-graphql --show-usage
# first and last should behave the same
{
  transactionBlocks {                                       # 1
    edges {                                                 # 1
      txns: node {                                          # 20
        digest                                              # 20
        first: expiration {                                 # 20
          checkpoints(first: 20) {                          # 20
            edges {                                         # 20
              node {                                        # 400
                sequenceNumber                              # 400
              }
            }
          }
        }
        last: expiration {                                  # 20
          checkpoints(last: 20) {                           # 20
            edges {                                         # 20
              node {                                        # 400
                sequenceNumber                              # 400
              }
            }
          }
        }
      }
    }
  }
}

//# run-graphql --show-usage
# edges incur additional cost over nodes, because of the extra level
# of nesting
{
  transactionBlocks {                                       # 1
    nodes {                                                 # 1
      digest                                                # 20
      first: expiration {                                   # 20
        checkpoints(first: 20) {                            # 20
          edges {                                           # 20
            node {                                          # 400
              sequenceNumber                                # 400
            }
          }
        }
      }
      last: expiration {                                    # 20
        checkpoints(last: 20) {                             # 20
          edges {                                           # 20
            node {                                          # 400
              sequenceNumber                                # 400
            }
          }
        }
      }
    }
  }
}

//# run-graphql --show-usage
# example lifted from complex query at
# https://docs.github.com/en/graphql/overview/rate-limits-and-node-limits-for-the-graphql-api#node-limit
# our costing will be different since we consider all nodes
{
  transactionBlocks(first: 50) {                            # 1
    edges {                                                 # 1
      txns: node {                                          # 50
        digest                                              # 50
        a: expiration {                                     # 50
          checkpoints(last: 20) {                           # 50
            edges {                                         # 50
              node {                                        # 50 * 20
                transactionBlocks(first: 10) {              # 50 * 20
                  edges {                                   # 50 * 20
                    node {                                  # 50 * 20 * 10
                      digest                                # 50 * 20 * 10
                    }
                  }
                }
              }
            }
          }
        }
        b: expiration {                                     # 50
          checkpoints(first: 20) {                          # 50
            edges {                                         # 50
              node {                                        # 50 * 20
                transactionBlocks(last: 10) {               # 50 * 20
                  edges {                                   # 50 * 20
                    node {                                  # 50 * 20 * 10
                      digest                                # 50 * 20 * 10
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
  events(last: 10) {                                        # 1
    edges {                                                 # 1
      node {                                                # 10
        timestamp                                           # 10
      }
    }
  }
}

//# run-graphql --show-usage
# Null value for variable passed to limit will use default_page_size
query NullVariableForLimit($howMany: Int) {
  transactionBlocks(last: $howMany) {                       # 1
    edges {                                                 # 1
      node {                                                # 20
        digest                                              # 20
        a: expiration {                                     # 20
          checkpoints {                                     # 20
            edges {                                         # 20
              node {                                        # 400
                transactionBlocks(first: $howMany) {        # 400
                  edges {                                   # 400
                    node {                                  # 8000
                      digest                                # 8000
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}

//# run-graphql --show-usage
# Connection detection needs to be resilient to connection fields
# being obscured by fragments.
fragment Nodes on TransactionBlockConnection {
  nodes {
    digest
  }
}

{
  fragmentSpread: transactionBlocks {                       # 1
    ...Nodes                                                # 1 + 20
  }

  inlineFragment: transactionBlocks {                       # 1
    ... on TransactionBlockConnection {
      nodes {                                               # 1
        digest                                              # 20
      }
    }
  }
}

//# run-graphql --show-usage

# error state - can't use first and last together, but we will use the
# max of the two for output node estimation
{
  transactionBlocks(first: 20, last: 30) {                  # 1
    edges {                                                 # 1
      node {                                                # 30
        digest                                              # 30
      }
    }
  }
}

//# run-graphql --show-usage
# error state - overflow u64
{
  transactionBlocks(first: 36893488147419103000) {
    edges {
      node {
        digest
      }
    }
  }
}

//# run-graphql --show-usage
# error state, overflow u32
{
  transactionBlocks(first: 4294967297) {
    edges {
      node {
        digest
      }
    }
  }
}
