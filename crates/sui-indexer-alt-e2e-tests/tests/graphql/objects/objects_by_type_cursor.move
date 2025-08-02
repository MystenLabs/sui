// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

// Create first batch of tables before checkpoint 1
//# programmable --sender A --inputs @A
//> 0: sui::table::new<u8, u8>();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::table::new<u8, u8>();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::table::new<u8, u8>();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::table::new<u64, u64>();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::table::new<u64, u64>();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::table::new<u64, u64>();
//> 1: TransferObjects([Result(0)], Input(0))

//# create-checkpoint

// Create second batch of tables before checkpoint 2
//# programmable --sender A --inputs @A
//> 0: sui::table::new<u8, u8>();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::table::new<u8, u8>();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::table::new<u8, u8>();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::table::new<u64, u64>();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::table::new<u64, u64>();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::table::new<u64, u64>();
//> 1: TransferObjects([Result(0)], Input(0))

//# create-checkpoint

// Create third batch of tables before checkpoint 3
//# programmable --sender A --inputs @A
//> 0: sui::table::new<u8, u8>();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::table::new<u8, u8>();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::table::new<u8, u8>();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::table::new<u64, u64>();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::table::new<u64, u64>();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::table::new<u64, u64>();
//> 1: TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# run-graphql
{ # Display all the objects created, so we can see their addresses and versions
  cp1: multiGetObjects(keys: [
    { address: "@{obj_1_0}" },
    { address: "@{obj_2_0}" },
    { address: "@{obj_3_0}" },
    { address: "@{obj_4_0}" },
    { address: "@{obj_5_0}" },
    { address: "@{obj_6_0}" },
  ]) { ...Obj }

  cp2: multiGetObjects(keys: [
    { address: "@{obj_8_0}" },
    { address: "@{obj_9_0}" },
    { address: "@{obj_10_0}" },
    { address: "@{obj_11_0}" },
    { address: "@{obj_12_0}" },
    { address: "@{obj_13_0}" },
  ]) { ...Obj }

  cp3: multiGetObjects(keys: [
    { address: "@{obj_15_0}" },
    { address: "@{obj_16_0}" },
    { address: "@{obj_17_0}" },
    { address: "@{obj_18_0}" },
    { address: "@{obj_19_0}" },
    { address: "@{obj_20_0}" },
  ]) { ...Obj }
}

fragment Obj on Object {
  address
  version
}

//# run-graphql
{ # Display all tables to identify their pagination order
  objects(filter: {type: "0x2::table::Table"}) {
    pageInfo { hasNextPage hasPreviousPage }
    nodes {
      address
      version
    }
  }
}

//# run-graphql
{ # Test unqualified Table pagination with first
  objects(filter: {type: "0x2::table::Table"}, first: 5) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(3,bin(0x2::table::Table<u8,u8>,@{obj_1_0}))
{ # Test unqualified Table pagination with after + first
  objects(
    filter: {type: "0x2::table::Table"}
    after: "@{cursor_0}"
    first: 3
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(3,bin(0x2::table::Table<u8,u8>,@{obj_8_0}))
{ # Test unqualified Table pagination with after + last
  objects(
    filter: {type: "0x2::table::Table"}
    after: "@{cursor_0}"
    last: 4
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(3,bin(0x2::table::Table<u8,u8>,@{obj_17_0}))
{ # Test unqualified Table pagination with before + first
  objects(
    filter: {type: "0x2::table::Table"}
    before: "@{cursor_0}"
    first: 3
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(3,bin(0x2::table::Table<u64,u64>,@{obj_13_0}))
{ # Test unqualified Table pagination with before + last
  objects(
    filter: {type: "0x2::table::Table"}
    before: "@{cursor_0}"
    last: 4
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(3,bin(0x2::table::Table<u8,u8>,@{obj_3_0})) bcs(3,bin(0x2::table::Table<u64,u64>,@{obj_11_0}))
{ # Test unqualified Table with both after and before cursors
  objects(
    filter: {type: "0x2::table::Table"}
    after: "@{cursor_0}"
    before: "@{cursor_1}"
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(3,bin(0x2::table::Table<u8,u8>,@{obj_2_0}))
{ # Test Table<u8, u8> pagination with after + first
  objects(
    filter: {type: "0x2::table::Table<u8, u8>"}
    after: "@{cursor_0}"
    first: 3
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(3,bin(0x2::table::Table<u8,u8>,@{obj_1_0}))
{ # Test Table<u8, u8> pagination with after + last
  objects(
    filter: {type: "0x2::table::Table<u8, u8>"}
    after: "@{cursor_0}"
    last: 2
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(3,bin(0x2::table::Table<u8,u8>,@{obj_17_0}))
{ # Test Table<u8, u8> pagination with before + first
  objects(
    filter: {type: "0x2::table::Table<u8, u8>"}
    before: "@{cursor_0}"
    first: 2
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(3,bin(0x2::table::Table<u8,u8>,@{obj_16_0}))
{ # Test Table<u8, u8> pagination with before + last
  objects(
    filter: {type: "0x2::table::Table<u8, u8>"}
    before: "@{cursor_0}"
    last: 3
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(3,bin(0x2::table::Table<u8,u8>,@{obj_1_0})) bcs(3,bin(0x2::table::Table<u8,u8>,@{obj_10_0}))
{ # Test Table<u8, u8> with both after and before cursors
  objects(
    filter: {type: "0x2::table::Table<u8, u8>"}
    after: "@{cursor_0}"
    before: "@{cursor_1}"
    first: 2
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(3,bin(0x2::table::Table<u64,u64>,@{obj_5_0}))
{ # Test Table<u64, u64> pagination with after + first
  objects(
    filter: {type: "0x2::table::Table<u64, u64>"}
    after: "@{cursor_0}"
    first: 3
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(3,bin(0x2::table::Table<u64,u64>,@{obj_4_0}))
{ # Test Table<u64, u64> pagination with after + last
  objects(
    filter: {type: "0x2::table::Table<u64, u64>"}
    after: "@{cursor_0}"
    last: 2
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(3,bin(0x2::table::Table<u64,u64>,@{obj_20_0}))
{ # Test Table<u64, u64> pagination with before + first
  objects(
    filter: {type: "0x2::table::Table<u64, u64>"}
    before: "@{cursor_0}"
    first: 2
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(3,bin(0x2::table::Table<u64,u64>,@{obj_19_0}))
{ # Test Table<u64, u64> pagination with before + last
  objects(
    filter: {type: "0x2::table::Table<u64, u64>"}
    before: "@{cursor_0}"
    last: 3
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(3,bin(0x2::table::Table<u64,u64>,@{obj_4_0})) bcs(3,bin(0x2::table::Table<u64,u64>,@{obj_13_0}))
{ # Test Table<u64, u64> with both after and before cursors
  objects(
    filter: {type: "0x2::table::Table<u64, u64>"}
    after: "@{cursor_0}"
    before: "@{cursor_1}"
    first: 2
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(3,bin(0x2::table::Table<u8,u8>,@{obj_8_0}))
{ # Test cursor with only after (no limit) for Table<u8, u8>
  objects(
    filter: {type: "0x2::table::Table<u8, u8>"}
    after: "@{cursor_0}"
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(3,bin(0x2::table::Table<u64,u64>,@{obj_12_0}))
{ # Test cursor with only before (no limit) for Table<u64, u64>
  objects(
    filter: {type: "0x2::table::Table<u64, u64>"}
    before: "@{cursor_0}"
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql
{ # Checkpoint 2: Query all tables at historical checkpoint
  checkpoint(sequenceNumber: 2) {
    query {
      objects(filter: {type: "0x2::table::Table"}) {
        pageInfo {
          hasNextPage
          hasPreviousPage
        }
        nodes {
          address
          version
        }
      }
    }
  }
}

//# run-graphql --cursors bcs(2,bin(0x2::table::Table<u8,u8>,@{obj_2_0}))
{ # Checkpoint 2: Test pagination with after + first using object from checkpoint 1
  objects(
    filter: {type: "0x2::table::Table<u8, u8>"}
    after: "@{cursor_0}"
    first: 3
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(2,bin(0x2::table::Table<u64,u64>,@{obj_11_0}))
{ # Checkpoint 2: Test pagination with before + last using object from checkpoint 2
  objects(
    filter: {type: "0x2::table::Table<u64, u64>"}
    before: "@{cursor_0}"
    last: 2
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(2,bin(0x2::table::Table<u8,u8>,@{obj_1_0})) bcs(2,bin(0x2::table::Table<u64,u64>,@{obj_11_0}))
{ # Checkpoint 2: Test range query with both cursors
  objects(
    filter: {type: "0x2::table::Table"}
    after: "@{cursor_0}"
    before: "@{cursor_1}"
    first: 4
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(2,bin(0x2::table::Table<u8,u8>,@{obj_1_0}))
{ # Checkpoint 2: Test after + last combination
  objects(
    filter: {type: "0x2::table::Table<u8, u8>"}
    after: "@{cursor_0}"
    last: 2
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(2,bin(0x2::table::Table<u64,u64>,@{obj_12_0}))
{ # Checkpoint 2: Test before + first combination
  objects(
    filter: {type: "0x2::table::Table<u64, u64>"}
    before: "@{cursor_0}"
    first: 3
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}

//# run-graphql --cursors bcs(2,bin(0x2::table::Table<u8,u8>,@{obj_1_0})) bcs(3,bin(0x2::table::Table<u8,u8>,@{obj_17_0}))
{ # Invalid cursors
  objects(
    filter: {type: "0x2::table::Table"}
    after: "@{cursor_0}"
    before: "@{cursor_1}"
  ) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
      address
      version
    }
  }
}
