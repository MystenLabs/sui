// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

// Various error cases related to dynamic field names.

//# run-graphql --cursors bcs(42u64)
{ # Missing type information
  address(address: "@{obj_0_0}") {
    dynamicField(name: { bcs: "@{cursor_0}" }) {
      address
    }
  }
}

//# run-graphql --cursors bcs(42u64)
{ # Missing BCS
  address(address: "@{obj_0_0}") {
    dynamicField(name: { type: "u64" }) {
      address
    }
  }
}

//# run-graphql --cursors bcs(42u64)
{ # Mixing literal and type/BCS
  address(address: "@{obj_0_0}") {
    dynamicField(name: { type: "u64", bcs: "@{cursor_0}", literal: "42u64" }) {
      address
    }
  }
}

//# run-graphql
{ # Literal parsing error
  address(address: "@{obj_0_0}") {
    dynamicField(name: { literal: "'hello" }) {
      address
    }
  }
}

//# run-graphql
{ # Field access
  address(address: "@{obj_0_0}") {
    dynamicField(name: { literal: "0x1::ascii::String(foo)" }) {
      address
    }
  }
}
