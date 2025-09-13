// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 47 --accounts A --addresses test=0x0 --simulator

//# publish --sender A --upgradeable
module test::clever_error_pre_v48 {
    #[error]
    const ESimpleError: vector<u8> = b"Clever error from v1";

    public entry fun test_clever_error() {
        abort ESimpleError
    }
}

//# upgrade --package test --upgrade-capability 1,1 --sender A
module test::clever_error_pre_v48 {
    #[error]
    const ESimpleError: vector<u8> = b"Clever error from v2";
    
    #[error]
    const ENewError: vector<u8> = b"New error in v2";

    public entry fun test_clever_error() {
        abort ESimpleError
    }
    
    public entry fun test_new_error() {
        abort ENewError
    }
}

//# run test::clever_error_pre_v48::test_new_error --sender A

//# create-checkpoint

//# run-graphql
{
  # Display package address to verify the upgraded package is being used in executionError
  originalPackage: object(address: "@{obj_1_0}") {
    address
  }

  upgradedPackage: object(address: "@{obj_2_0}") {
    address
  }

  cleverErrorPreV48: transactionEffects(digest: "@{digest_3}") {
    executionError {
      abortCode
      sourceLineNumber
      instructionOffset
      identifier
      constant
      message
      module {
        name
        package { address }
      }
      function {
        name
        module { name }
      }
    }
  }
}
