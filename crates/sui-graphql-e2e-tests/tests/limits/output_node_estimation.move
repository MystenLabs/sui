// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 48 --addresses A=0x42 --simulator

//# run-graphql --show-usage
# pageInfo does not inherit connection's weights
{
  transactionBlocks(first: 20) {
    pageInfo {
      hasPreviousPage
    }
  }
}

//# run-graphql --show-usage
# if connection does not have 'first' or 'last' set, use default_page_size (20)
{
  transactionBlocks {
    edges {
      node {
        digest
      }
    }
  }
}

//# run-graphql --show-usage
# build on previous example with nested connection
{
  checkpoints {
    nodes {
      transactionBlocks {
        edges {
          txns: node {
            digest
          }
        }
      }
    }
  }
}

//# run-graphql --show-usage
# handles 1
{
  checkpoints {
    nodes {
      notOne: transactionBlocks {
        edges {
          txns: node {
            digest
          }
        }
      }
      isOne: transactionBlocks(first: 1) {
        edges {
          txns: node {
            digest
          }
        }
      }
    }
  }
}

//# run-graphql --show-usage
# handles 0
{
  checkpoints {
    nodes {
      notZero: transactionBlocks {
        edges {
          txns: node {
            digest
          }
        }
      }
      isZero: transactionBlocks(first: 0) {
        edges {
          txns: node {
            digest
          }
        }
      }
    }
  }
}

//# run-graphql --show-usage
# if connection does have 'first' set, use it
{
  transactionBlocks(first: 1) {
    edges {
      txns: node {
        digest
      }
    }
  }
}

//# run-graphql --show-usage
# if connection does have 'last' set, use it
{
  transactionBlocks(last: 1) {
    edges {
      txns: node {
        digest
      }
    }
  }
}

//# run-graphql --show-usage
# first and last should behave the same
{
  transactionBlocks {
    edges {
      txns: node {
        digest
        first: expiration {
          checkpoints(first: 20) {
            edges {
              node {
                sequenceNumber
              }
            }
          }
        }
        last: expiration {
          checkpoints(last: 20) {
            edges {
              node {
                sequenceNumber
              }
            }
          }
        }
      }
    }
  }
}

//# run-graphql --show-usage
# edges incur additional cost over nodes
{
  transactionBlocks {
    nodes {
      digest
      first: expiration { # 80 cumulative
        checkpoints(first: 20) {
          edges {
            node {
              sequenceNumber
            }
          }
        }
      } # 1680 cumulative
      last: expiration { # 20 + 1680 = 1700 cumulative
        checkpoints(last: 20) {
          edges {
            node {
              sequenceNumber
            }
          }
        } # another 1600, 3300 cumulative
      }
    }
  }
}

//# run-graphql --show-usage
# example lifted from complex query at
# https://docs.github.com/en/graphql/overview/rate-limits-and-node-limits-for-the-graphql-api#node-limit
# our costing will be different since we consider all nodes
{
  transactionBlocks(first: 50) { # 50, 50
    edges { # 50, 100
      txns: node { # 50, 150
        digest # 50, 200
        a: expiration { # 50, 250
          checkpoints(last: 20) { # 50 * 20 = 1000, 1250
            edges { # 1000, 2250
              node { # 1000, 3250
                transactionBlocks(first: 10) { # 50 * 20 * 10 = 10000, 13250
                  edges { # 10000, 23250
                    node { # 10000, 33250
                      digest # 10000, 43250
                    }
                  }
                }
              }
            }
          }
        }
        b: expiration { # 50, 43300
          checkpoints(first: 20) { # 50 * 20 = 1000, 44300
            edges { # 1000, 45300
              node { # 1000, 46300
                transactionBlocks(last: 10) { # 50 * 20 * 10 = 10000, 56300
                  edges { # 10000, 66300
                    node { # 10000, 76300
                      digest # 10000, 86300
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
  events(last: 10) { # 10
    edges {
      node {
        timestamp
      }
    }
  } # 40, 86340
}

//# run-graphql --show-usage
# Null value for variable passed to limit will use default_page_size
query NullVariableForLimit($howMany: Int) {
  transactionBlocks(last: $howMany) { # 20, 20
    edges { # 20, 40
      node { # 20, 60
        digest # 20, 80
        a: expiration { # 20, 100
          checkpoints { # 20 * 20， 500
            edges { # 400, 900
              node { # 400, 1300
                transactionBlocks(first: $howMany) { # 20 * 20 * 20 = 8000， 9300
                  edges { # 8000, 17300
                    node { # 8000, 25300
                      digest # 8000, 33300
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
# error state - can't use first and last together
{
  transactionBlocks(first: 20, last: 30) {
    edges {
      node {
        digest
      }
    }
  }
}

//# run-graphql --show-usage
# error state - exceed max integer
{
  transactionBlocks(first: 36893488147419103000) {
    edges {
      node {
        digest
      }
    }
  }
}
