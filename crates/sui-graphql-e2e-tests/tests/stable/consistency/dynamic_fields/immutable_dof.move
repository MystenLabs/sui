// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// 1. create Parent1 (2)
// 2. create Child1 (3)
// 3. create Child2 (4)
// 4. add Child1 to Parent1 -> Parent1 (5), Child1 (5)
// 5. add Child2 as a nested child object by borrowing Parent1.Child1 -> Parent1 (6), Child1 (5), Child2 (6)
// 6. make Child1 immutable

// dynamic fields rooted on parent
// Child1 (5) should have a parent
// Child1 (6) does not exist
// Child1 (7) should show as an Immutable object

// Verify that Parent1 (6) -> Child1 (5) -> Child2 (6)

//# init --protocol-version 51 --addresses Test=0x0 --accounts A --simulator

//# publish
module Test::M1 {
    use sui::dynamic_object_field as ofield;

    public struct Parent has key, store {
        id: UID,
        count: u64
    }

    public struct Child has key, store {
        id: UID,
        count: u64,
    }

    public entry fun parent(recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Parent { id: object::new(ctx), count: 0 },
            recipient
        )
    }

    public entry fun mutate_parent(parent: &mut Parent) {
        parent.count = parent.count + 42;
    }

    public entry fun child(recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Child { id: object::new(ctx), count: 0 },
            recipient
        )
    }

    public fun add_child(parent: &mut Parent, child: Child, name: u64) {
        ofield::add(&mut parent.id, name, child);
    }

    public fun add_nested_child(parent: &mut Parent, child_name: u64, nested_child: Child, nested_child_name: u64) {
        let child: &mut Child = ofield::borrow_mut(&mut parent.id, child_name);
        ofield::add(&mut child.id, nested_child_name, nested_child);
    }

    public fun reclaim_child(parent: &mut Parent, name: u64): Child {
        ofield::remove(&mut parent.id, name)
    }

    public fun reclaim_and_freeze_child(parent: &mut Parent, name: u64) {
        transfer::public_freeze_object(reclaim_child(parent, name))
    }
}

//# run Test::M1::parent --sender A --args @A

//# run Test::M1::child --sender A --args @A

//# run Test::M1::child --sender A --args @A

//# run Test::M1::add_child --sender A --args object(2,0) object(3,0) 42

//# run Test::M1::add_nested_child --sender A --args object(2,0) 42 object(4,0) 420

//# run Test::M1::reclaim_and_freeze_child --sender A --args object(2,0) 42

//# create-checkpoint

//# run-graphql
{
  object(address: "@{obj_2_0}", version: 5) {
    dynamicFields {
      nodes {
        value {
          ... on MoveObject {
            address
            version
            contents {
              json
            }
            dynamicFields {
              nodes {
                value {
                  ... on MoveObject {
                    address
                    version
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

//# run-graphql
{
  object(address: "@{obj_2_0}", version: 6) {
    dynamicFields {
      nodes {
        value {
          ... on MoveObject {
            address
            version
            contents {
              json
            }
            dynamicFields {
              nodes {
                value {
                  ... on MoveObject {
                    address
                    version
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

//# run-graphql
{
  object(address: "@{obj_2_0}", version: 7) {
    dynamicFields {
      nodes {
        value {
          ... on MoveObject {
            address
            version
            contents {
              json
            }
            dynamicFields {
              nodes {
                value {
                  ... on MoveObject {
                    address
                    version
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

//# run-graphql
{
  object(address: "@{obj_2_0}", version: 8) {
    dynamicFields {
      nodes {
        value {
          ... on MoveObject {
            address
            version
            contents {
              json
            }
            dynamicFields {
              nodes {
                value {
                  ... on MoveObject {
                    address
                    version
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

//# run-graphql
{
  object(address: "@{obj_3_0}", version: 5) {
    owner {
      ... on Immutable {
        _
      }
      ... on Parent {
        parent {
          address
        }
      }
    }
    dynamicFields {
      nodes {
        value {
          ... on MoveObject {
            address
            version
            contents {
              json
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{
  object(address: "@{obj_3_0}", version: 6) {
    dynamicFields {
      nodes {
        value {
          ... on MoveObject {
            address
            version
            contents {
              json
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{
  object(address: "@{obj_3_0}", version: 7) {
    owner {
      ... on Immutable {
        _
      }
      ... on Parent {
        parent {
          address
        }
      }
    }
    dynamicFields {
      nodes {
        value {
          ... on MoveObject {
            address
            version
            contents {
              json
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{
  object(address: "@{obj_3_0}") {
    owner {
      ... on Immutable {
        _
      }
      ... on Parent {
        parent {
          address
        }
      }
    }
    dynamicFields {
      nodes {
        value {
          ... on MoveObject {
            address
            version
            contents {
              json
            }
          }
        }
      }
    }
  }
}
