// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Simple test case to check that object pagination is bounded by the checkpoint in the cursor. Use
// one object as a cursor to view the other object that gets mutated per checkpoint. The ordering is
// consistent even if the cursor object is mutated.

//# init --protocol-version 51 --addresses Test=0x0 --accounts A B --simulator

//# publish
module Test::M1 {
    public struct Object has key, store {
        id: UID,
        value: u64,
    }

    public entry fun create(value: u64, recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Object { id: object::new(ctx), value },
            recipient
        )
    }

    public entry fun update(value: u64, o: &mut Object) {
        o.value = value;
    }
}

//# run Test::M1::create --args 0 @A

//# run Test::M1::create --args 1 @A

//# run Test::M1::update --sender A --args 100 object(2,0)

//# create-checkpoint

//# run-graphql --cursors bcs(@{obj_3_0},@{highest_checkpoint})
{
  after_obj_3_0: address(address: "@{A}") {
    objects(filter: {type: "@{Test}"}, after: "@{cursor_0}") {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
      }
    }
  }
  before_obj_3_0: address(address: "@{A}") {
    objects(filter: {type: "@{Test}"}, before: "@{cursor_0}") {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
      }
    }
  }
}

//# run Test::M1::update --sender A --args 200 object(2,0)

//# create-checkpoint

//# run-graphql --cursors bcs(@{obj_3_0},1)
# This query should yield the same results as the previous one.
{
  after_obj_3_0_chkpt_1: address(address: "@{A}") {
    objects(filter: {type: "@{Test}"}, after: "@{cursor_0}") {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
      }
    }
  }
  before_obj_3_0_chkpt_1: address(address: "@{A}") {
    objects(filter: {type: "@{Test}"}, before: "@{cursor_0}") {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
      }
    }
  }
}

//# run-graphql --cursors bcs(@{obj_3_0},2)
{
  address(address: "@{A}") {
    objects(filter: {type: "@{Test}"}) {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
      }
    }
  }
  after_obj_3_0_chkpt_2: address(address: "@{A}") {
    consistent_with_above: objects(filter: {type: "@{Test}"}, after: "@{cursor_0}") {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
        owner {
          ... on AddressOwner {
            owner {
              objects(filter: {type: "@{Test}"}) {
                nodes {
                  version
                  contents {
                    type {
                      repr
                    }
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
  before_obj_3_0_chkpt_2: address(address: "@{A}") {
    consistent_with_above: objects(filter: {type: "@{Test}"}, before: "@{cursor_0}") {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
        owner {
          ... on AddressOwner {
            owner {
              objects(filter: {type: "@{Test}"}) {
                nodes {
                  version
                  contents {
                    type {
                      repr
                    }
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

//# run Test::M1::update --sender A --args 300 object(3,0)

//# create-checkpoint

//# run-graphql --cursors bcs(@{obj_3_0},2)
# This query should yield the same results as the previous one.
{
    after_obj_3_0_chkpt_2: address(address: "@{A}") {
    objects(filter: {type: "@{Test}"}, after: "@{cursor_0}") {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
        this_should_differ: owner {
          ... on AddressOwner {
            owner {
              objects(filter: {type: "@{Test}"}) {
                nodes {
                  version
                  contents {
                    type {
                      repr
                    }
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
  before_obj_3_0_chkpt_2: address(address: "@{A}") {
    objects(filter: {type: "@{Test}"}, before: "@{cursor_0}") {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
        this_should_differ: owner {
          ... on AddressOwner {
            owner {
              objects(filter: {type: "@{Test}"}) {
                nodes {
                  version
                  contents {
                    type {
                      repr
                    }
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

//# run-graphql --cursors bcs(@{obj_3_0},3)
{
  address(address: "@{A}") {
    objects(filter: {type: "@{Test}"}) {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
      }
    }
  }
  after_obj_3_0_chkpt_3: address(address: "@{A}") {
    consistent_with_above: objects(filter: {type: "@{Test}"}, after: "@{cursor_0}") {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
        owner {
          ... on AddressOwner {
            owner {
              objects(filter: {type: "@{Test}"}) {
                nodes {
                  version
                  contents {
                    type {
                      repr
                    }
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
  before_obj_3_0_chkpt_3: address(address: "@{A}") {
    consistent_with_above: objects(filter: {type: "@{Test}"}, before: "@{cursor_0}") {
      nodes {
        version
        contents {
          type {
            repr
          }
          json
        }
        owner {
          ... on AddressOwner {
            owner {
              objects(filter: {type: "@{Test}"}) {
                nodes {
                  version
                  contents {
                    type {
                      repr
                    }
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
