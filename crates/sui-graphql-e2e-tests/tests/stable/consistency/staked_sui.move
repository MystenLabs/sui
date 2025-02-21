// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --simulator --accounts C

//# run-graphql
{
  address(address: "@{C}") {
    stakedSuis {
      edges {
        cursor
        node {
          principal
        }
      }
    }
  }
}

//# programmable --sender C --inputs 10000000000 @C
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# run 0x3::sui_system::request_add_stake --args object(0x5) object(2,0) @validator_0 --sender C

//# create-checkpoint

//# advance-epoch

//# programmable --sender C --inputs 10000000000 @C
//> SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# run 0x3::sui_system::request_add_stake --args object(0x5) object(6,0) @validator_0 --sender C

//# create-checkpoint

//# advance-epoch

//# view-object 3,1

//# view-object 7,0

//# run-graphql
{
  address(address: "@{C}") {
    stakedSuis {
      edges {
        cursor
        node {
          principal
        }
      }
    }
  }
}

//# run-graphql --cursors bcs(@{obj_3_1},1) bcs(@{obj_7_0},1)
# Even though there is a stake created after the initial one, the cursor locks the upper bound to
# checkpoint 1 - at that point in time, we did not have any additional stakes.
{
  no_coins_after_obj_3_1_chkpt_1: address(address: "@{C}") {
    stakedSuis(after: "@{cursor_0}") {
      edges {
        cursor
        node {
          principal
        }
      }
    }
  }
  no_coins_before_obj_3_1_chkpt_1: address(address: "@{C}") {
    stakedSuis(before: "@{cursor_0}") {
      edges {
        cursor
        node {
          principal
        }
      }
    }
  }
  no_coins_after_obj_7_0_chkpt_1: address(address: "@{C}") {
    stakedSuis(after: "@{cursor_0}") {
      edges {
        cursor
        node {
          principal
        }
      }
    }
  }
  no_coins_before_obj_7_0_chkpt_1: address(address: "@{C}") {
    stakedSuis(before: "@{cursor_0}") {
      edges {
        cursor
        node {
          principal
        }
      }
    }
  }
}

//# run-graphql --cursors bcs(@{obj_3_1},3) bcs(@{obj_7_0},3)
# The second stake was created at checkpoint 3, and thus will be visible.
{
  coins_after_obj_3_1_chkpt_3: address(address: "@{C}") {
    stakedSuis(after: "@{cursor_0}") {
      edges {
        cursor
        node {
          principal
        }
      }
    }
  }
  coins_before_obj_3_1_chkpt_3: address(address: "@{C}") {
    stakedSuis(before: "@{cursor_0}") {
      edges {
        cursor
        node {
          principal
        }
      }
    }
  }
  coins_after_obj_7_0_chkpt_3: address(address: "@{C}") {
    stakedSuis(after: "@{cursor_1}") {
      edges {
        cursor
        node {
          principal
        }
      }
    }
  }
  coins_before_obj_7_0_chkpt_3: address(address: "@{C}") {
    stakedSuis(before: "@{cursor_1}") {
      edges {
        cursor
        node {
          principal
        }
      }
    }
  }
}
