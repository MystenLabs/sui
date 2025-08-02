// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

//# run-graphql
{ # ADDRESS owner kind with owner - not supported yet
  objects(filter: {ownerKind: ADDRESS, owner: "0x42"}) {
    nodes {
      digest
    }
  }
}

//# run-graphql
{ # OBJECT owner kind with owner - not supported yet
  objects(filter: {ownerKind: OBJECT, owner: "0x42"}) {
    nodes {
      digest
    }
  }
}

//# run-graphql
{ # Owner without ownerKind (defaults to ADDRESS) - not supported yet
  objects(filter: {owner: "0x42"}) {
    nodes {
      digest
    }
  }
}

//# run-graphql
{ # ADDRESS owner kind with owner and type filter - not supported yet
  objects(filter: {ownerKind: ADDRESS, owner: "0x42", type: "0x2::coin::Coin"}) {
    nodes {
      digest
    }
  }
}

//# run-graphql
{ # OBJECT owner kind with owner and type filter - not supported yet
  objects(filter: {ownerKind: OBJECT, owner: "0x42", type: "0x2::coin::Coin"}) {
    nodes {
      digest
    }
  }
}

//# run-graphql
{ # SHARED owner kind with type - not supported yet
  objects(filter: {ownerKind: SHARED, type: "0x2::coin::Coin"}) {
    nodes {
      digest
    }
  }
}

//# run-graphql
{ # IMMUTABLE owner kind with type - not supported yet
  objects(filter: {ownerKind: IMMUTABLE, type: "0x2::coin::Coin"}) {
    nodes {
      digest
    }
  }
}