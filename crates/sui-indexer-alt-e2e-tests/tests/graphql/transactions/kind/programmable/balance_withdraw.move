// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// `BalanceWithdraw` inputs. For an allowance-sourced withdrawal the funder and
// the allowance are addresses, and `asTransactionObject` resolves the allowance
// as the transaction changed it, without an explicit digest. A plain sender
// withdrawal has neither.

//# init --accounts A B --simulator

//# programmable --sender A --inputs 5000 @A
// Fund A's (the funder) address balance.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

//# programmable --sender A --inputs b"gql" @B vector[1000u256] vector[] vector[99999999999999]
// A issues an allowance to B: 1000 lifetime cap.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<sui::sui::SUI>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(400,@A,object(3,0)) mutshared(3,0) immshared(6) @B
// B spends 400 of A's funds through the allowance.
//> 0: sui::allowance::balance_spend<sui::sui::SUI>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(3));

//# create-checkpoint

//# run-graphql
{
  transaction(digest: "@{digest_4}") {
    kind {
      __typename
      ... on ProgrammableTransaction {
        inputs(first: 10) {
          nodes {
            __typename
            ... on BalanceWithdraw {
              withdrawFrom
              reservation {
                ... on WithdrawMaxAmountU64 {
                  amount
                }
              }
              funder {
                address
              }
              allowance {
                address
                asTransactionObject {
                  __typename
                  ... on ObjectChange {
                    address
                    inputState {
                      version
                    }
                    outputState {
                      version
                      asMoveObject {
                        contents {
                          json
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
  }
}

//# programmable --sender B --inputs withdraw<sui::balance::Balance<sui::sui::SUI>>(100) @A
// B withdraws 100 from their own balance: a plain sender withdrawal.
//> 0: sui::balance::redeem_funds<sui::sui::SUI>(Input(0));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(200,@A,object(3,0)) mutshared(3,0) immshared(6) @B
// A second spend moves the allowance past the version the first spend wrote.
//> 0: sui::allowance::balance_spend<sui::sui::SUI>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(3));

//# create-checkpoint

//# run-graphql
{
  # A plain sender withdrawal has no funder and no allowance.
  transaction(digest: "@{digest_7}") {
    kind {
      ... on ProgrammableTransaction {
        inputs(first: 10) {
          nodes {
            __typename
            ... on BalanceWithdraw {
              withdrawFrom
              reservation {
                ... on WithdrawMaxAmountU64 {
                  amount
                }
              }
              funder {
                address
              }
              allowance {
                address
              }
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{
  # Queried after a later spend: `asObject` is the latest version, while
  # `asTransactionObject` stays at the versions this transaction saw, and an
  # explicit digest can point at another transaction, here the one that
  # created the allowance.
  transaction(digest: "@{digest_4}") {
    kind {
      ... on ProgrammableTransaction {
        inputs(first: 1) {
          nodes {
            ... on BalanceWithdraw {
              allowance {
                asObject {
                  version
                }
                asTransactionObject {
                  ... on ObjectChange {
                    inputState {
                      version
                    }
                    outputState {
                      version
                    }
                  }
                }
                created: asTransactionObject(transactionDigest: "@{digest_3}") {
                  ... on ObjectChange {
                    idCreated
                    outputState {
                      version
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
