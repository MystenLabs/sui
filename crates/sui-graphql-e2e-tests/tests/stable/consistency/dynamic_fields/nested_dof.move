// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// 1. create Parent1 (2)
// 2. create Child1 (3)
// 3. create Child2 (4)
// 4. add Child1 to Parent1 -> Parent1 (5), Child1 (5)
// 5. add Child2 as a nested child object by borrowing Parent1.Child1 -> Parent1 (6), Child1 (5), Child2 (6)
// 6. mutate child1 -> Parent1 (7), Child1 (7), Child2 (6)
// 7. mutate child2 through parent -> Parent1 (8), Child1 (7), Child2 (8)

// dynamic fields rooted on parent
// Parent(5) -> Child1 (5) -> None // add child1 as child to parent1
// Parent(6) -> Child1 (5) -> Child2 (6) // add child2 as a child to child1 by borrowing child1 from parent
// Parent(7) -> Child1 (7) -> Child2 (6) // mutate child1
// Parent(8) -> Child1 (7) -> Child2 (8) // mutate nested child2 by borrowing from child1

// query with Child1 as the root:
// Child1 (5) -> None
// Child1 (7) -> Child2 (6)

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

    public fun mutate_child_on_parent(parent: &mut Parent, child_name: u64) {
        let child: &mut Child = ofield::borrow_mut(&mut parent.id, child_name);
        child.count = child.count + 1;
    }

    public fun mutate_nested_child_on_parent(parent: &mut Parent, child_name: u64, nested_child_name: u64) {
        let child: &mut Child = ofield::borrow_mut(&mut parent.id, child_name);
        let nested_child: &mut Child = ofield::borrow_mut(&mut child.id, nested_child_name);
        nested_child.count = nested_child.count + 1;
    }
}

//# run Test::M1::parent --sender A --args @A

//# run Test::M1::child --sender A --args @A

//# run Test::M1::child --sender A --args @A

//# run Test::M1::add_child --sender A --args object(2,0) object(3,0) 42

//# run Test::M1::add_nested_child --sender A --args object(2,0) 42 object(4,0) 420

//# run Test::M1::mutate_child_on_parent --sender A --args object(2,0) 42

//# run Test::M1::mutate_nested_child_on_parent --sender A --args object(2,0) 42 420

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
