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

    #[error]
    const ECleverRaw: vector<address> = vector[@0x1, @0x2, @0x3];

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

    public entry fun abort_with_clever_raw() {
        abort ECleverRaw
    }

    public entry fun assert_failure() {
        assert!(false);
    }

    // MovePrimitiveRuntimeError test functions
    public entry fun arithmetic_underflow() {
        // Direct arithmetic error
        0 - 1;
    }

    public entry fun arithmetic_overflow() {
        // Direct arithmetic overflow
        18446744073709551615u64 + 1;
    }

    public entry fun division_by_zero() {
        // Direct division by zero
        1 / 0;
    }

    public entry fun vector_out_of_bounds() {
        // Direct vector access
        std::vector::borrow(&vector[0], 1);
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

//# run test::execution_error_tests::abort_with_clever_raw --sender B

//# run test::execution_error_tests::assert_failure --sender B

//# programmable --sender A --inputs @test
//> test::execution_error_tests::nonexistent_function()

//# run test::execution_error_tests::arithmetic_underflow --sender A

//# run test::execution_error_tests::arithmetic_overflow --sender B

//# run test::execution_error_tests::division_by_zero --sender A

//# run test::execution_error_tests::vector_out_of_bounds --sender B

//# create-checkpoint

//# run-graphql
{
  # Test execution_error on successful transaction (should be null)
  successTransaction: transactionEffects(digest: "@{digest_2}") {
    executionError {
      abortCode
      sourceLineNumber
      instructionOffset
      identifier
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
      instructionOffset
      identifier
    }
  }
  
  abort255: transactionEffects(digest: "@{digest_4}") {
    executionError {
      abortCode
      sourceLineNumber
      instructionOffset
      identifier
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
      instructionOffset
      identifier
    }
  }
  
  cleverU16: transactionEffects(digest: "@{digest_6}") {
    executionError {
      abortCode
      sourceLineNumber
      instructionOffset
      identifier
    }
  }
  
  cleverU64: transactionEffects(digest: "@{digest_7}") {
    executionError {
      abortCode
      sourceLineNumber
      instructionOffset
      identifier
    }
  }
  
  cleverAddress: transactionEffects(digest: "@{digest_8}") {
    executionError {
      abortCode
      sourceLineNumber
      instructionOffset
      identifier
    }
  }
  
  cleverString: transactionEffects(digest: "@{digest_9}") {
    executionError {
      abortCode
      sourceLineNumber
      instructionOffset
      identifier
    }
  }
  
  cleverWithCode: transactionEffects(digest: "@{digest_10}") {
    executionError {
      abortCode
      sourceLineNumber
      instructionOffset
      identifier
    }
  }

  cleverRaw: transactionEffects(digest: "@{digest_11}") {
    executionError {
      abortCode
      sourceLineNumber
      instructionOffset
      identifier
    }
  }
  
  assertFailure: transactionEffects(digest: "@{digest_12}") {
    executionError {
      abortCode
      sourceLineNumber
      instructionOffset
      identifier
    }
  }

  nonExistentFunction: transactionEffects(digest: "@{digest_13}") {
    executionError {
      abortCode
      sourceLineNumber
      instructionOffset
      identifier
    }
  }
}

//# run-graphql
{
  # Test MovePrimitiveRuntimeError cases
  arithmeticUnderflow: transactionEffects(digest: "@{digest_14}") {
    executionError {
      abortCode
      sourceLineNumber
      instructionOffset
      identifier
    }
  }
  
  arithmeticOverflow: transactionEffects(digest: "@{digest_15}") {
    executionError {
      abortCode
      sourceLineNumber
      instructionOffset
      identifier
    }
  }
  
  divisionByZero: transactionEffects(digest: "@{digest_16}") {
    executionError {
      abortCode
      sourceLineNumber
      instructionOffset
      identifier
    }
  }
  
  vectorOutOfBounds: transactionEffects(digest: "@{digest_17}") {
    executionError {
      abortCode
      sourceLineNumber
      instructionOffset
      identifier
    }
  }
}
