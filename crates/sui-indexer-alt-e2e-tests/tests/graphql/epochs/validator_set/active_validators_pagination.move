// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator --num-custom-validator-accounts 8

//# run-graphql --cursors 0 1 2 3 4 5 6 7
{
  epoch(epochId: 0) {
    epochId
    validatorSet {
      v_0_1_a: activeValidators(first: 2) { ...VC }
      v_0_1_b: activeValidators(first: 2, before: "@{cursor_2}") { ...VC }
      v_2_3_a: activeValidators(first: 2, after: "@{cursor_1}") { ...VC }
      v_2_3_b: activeValidators(first: 2, after: "@{cursor_1}", before: "@{cursor_4}") { ...VC }
      v_4_5_a: activeValidators(last: 2, before: "@{cursor_6}") { ...VC }
      v_4_5_b: activeValidators(last: 2, after: "@{cursor_3}", before: "@{cursor_6}") { ...VC }
      v_6_7_a: activeValidators(last: 2) { ...VC }
      v_6_7_b: activeValidators(last: 2, after: "@{cursor_5}") { ...VC }
    }
  }
}

fragment VC on ValidatorConnection {
  pageInfo {
    hasPreviousPage
    hasNextPage
    startCursor
    endCursor
  }
  nodes {
    name
  }
}
