// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

//# run-graphql
{ # ADDRESS owner kind without owner specified - should fail validation
  objects(filter: {ownerKind: ADDRESS}) {
    nodes {
      digest
    }
  }
}

//# run-graphql
{ # OBJECT owner kind without owner specified - should fail validation
  objects(filter: {ownerKind: OBJECT}) {
    nodes {
      digest
    }
  }
}

//# run-graphql
{ # SHARED owner kind with owner specified - should fail validation
  objects(filter: {ownerKind: SHARED, owner: "0x42"}) {
    nodes {
      digest
    }
  }
}

//# run-graphql
{ # IMMUTABLE owner kind with owner specified - should fail validation
  objects(filter: {ownerKind: IMMUTABLE, owner: "0x42"}) {
    nodes {
      digest
    }
  }
}

//# run-graphql
{ # SHARED owner kind without type specified - should fail validation
  objects(filter: {ownerKind: SHARED}) {
    nodes {
      digest
    }
  }
}

//# run-graphql
{ # IMMUTABLE owner kind without type specified - should fail validation
  objects(filter: {ownerKind: IMMUTABLE}) {
    nodes {
      digest
    }
  }
}

//# run-graphql
{ # Empty filter - should fail validation (when not allowed)
  objects(filter: {}) {
    nodes {
      digest
    }
  }
}
