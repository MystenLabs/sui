// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

//# publish
module test::execution_error_tests {
    // Different types of clever errors
    #[error]
    const ECleverU8: u8 = 10;

    #[error]
    const ECleverU16: u16 = 20;

    #[error]
    const ECleverU64: u64 = 100;

    #[error]
    const ECleverAddress: address = @0x42;

    #[error]
    const ECleverString: vector<u8> = b"This is a clever error message";

    #[error(code=15)]
    const ECleverWithCode: vector<u8> = b"Error with explicit code";

    /// Function that succeeds (for testing successful transactions)
    public entry fun success_function(_x: u64) {
        // Does nothing, just succeeds
    }

    /// Functions that abort with regular codes (no clever errors)
    public entry fun abort_with_42() {
        abort 42
    }

    public entry fun abort_with_255() {
        abort 255
    }

    /// Functions that abort with clever errors
    public entry fun abort_with_clever_u8() {
        abort ECleverU8
    }

    public entry fun abort_with_clever_u16() {
        abort ECleverU16
    }

    public entry fun abort_with_clever_u64() {
        abort ECleverU64
    }

    public entry fun abort_with_clever_address() {
        abort ECleverAddress
    }

    public entry fun abort_with_clever_string() {
        abort ECleverString
    }

    public entry fun abort_with_clever_code() {
        abort ECleverWithCode
    }

    public entry fun assert_failure() {
        assert!(false);
    }
}

//# run test::execution_error_tests::success_function --sender A --args 123

//# run test::execution_error_tests::abort_with_42 --sender A

//# run test::execution_error_tests::abort_with_255 --sender B

//# run test::execution_error_tests::abort_with_clever_u8 --sender B

//# run test::execution_error_tests::abort_with_clever_u16 --sender A

//# run test::execution_error_tests::abort_with_clever_u64 --sender B

//# run test::execution_error_tests::abort_with_clever_address --sender A

//# run test::execution_error_tests::abort_with_clever_string --sender B

//# run test::execution_error_tests::abort_with_clever_code --sender A

//# run test::execution_error_tests::assert_failure --sender B

//# programmable --sender A --inputs @test
//> test::execution_error_tests::nonexistent_function()

//# create-checkpoint

//# run-graphql
{
  # Test execution_error on successful transaction (should be null)
  successTransaction: transactionEffects(digest: "@{digest_2}") {
    executionError {
      abortCode
      sourceLineNumber
    }
  }
}

//# run-graphql
{
  # Test execution_error on non-clever abort codes
  abort42: transactionEffects(digest: "@{digest_3}") {
    executionError {
      abortCode
      sourceLineNumber
    }
  }
  
  abort255: transactionEffects(digest: "@{digest_4}") {
    executionError {
      abortCode
      sourceLineNumber
    }
  }
}

//# run-graphql
{
  # Test execution_error on various clever error types
  cleverU8: transactionEffects(digest: "@{digest_5}") {
    executionError {
      abortCode
      sourceLineNumber
    }
  }
  
  cleverU16: transactionEffects(digest: "@{digest_6}") {
    executionError {
      abortCode
      sourceLineNumber
    }
  }
  
  cleverU64: transactionEffects(digest: "@{digest_7}") {
    executionError {
      abortCode
      sourceLineNumber
    }
  }
  
  cleverAddress: transactionEffects(digest: "@{digest_8}") {
    executionError {
      abortCode
      sourceLineNumber
    }
  }
  
  cleverString: transactionEffects(digest: "@{digest_9}") {
    executionError {
      abortCode
      sourceLineNumber
    }
  }
  
  cleverWithCode: transactionEffects(digest: "@{digest_10}") {
    executionError {
      abortCode
      sourceLineNumber
    }
  }
  
  assertFailure: transactionEffects(digest: "@{digest_11}") {
    executionError {
      abortCode
      sourceLineNumber
    }
  }

  nonExistentFunction: transactionEffects(digest: "@{digest_12}") {
    executionError {
      abortCode
      sourceLineNumber
    }
  }
} 