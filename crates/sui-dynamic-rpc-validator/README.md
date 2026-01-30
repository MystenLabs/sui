# Sui Dynamic RPC Validator

This crate provides the interface and reference implementation for dynamic RPC validation in Sui validators.

## Overview

The Dynamic RPC Validator system allows validators to load validation logic from shared object files (.so/.dylib/.dll), enabling rapid deployment of validation checks without requiring a full validator software release.

## Components

### Library Interface

The library provides:
- `RpcMethod` enum - Type-safe RPC method identifiers
- `ValidationFn` - C FFI function signature for validation functions
- `DynamicRpcValidator` - Loader and manager for validation libraries
- `DynamicValidatorMetrics` - Prometheus metrics for monitoring

### Example Implementations

Two example validators are provided:

1. **Reference Validator** (`reference_validator`) - A simple validator that accepts all non-empty messages. Use this as a starting point for custom validators.

2. **Parsing Validator** (`parsing_validator`) - A validator that accepts messages only if they can be successfully parsed. This demonstrates how to implement validation logic that checks message structure:
   - Uses **Protobuf (prost)** decoding for `SubmitTransaction`, `WaitForEffects`, and `ValidatorHealth` requests
   - Uses **BCS** decoding for `ObjectInfo`, `TransactionInfo`, `Checkpoint`, and `SystemState` requests
   - For `SubmitTransaction`, also validates that inner transaction bytes can be BCS-decoded

## Building the Examples

### Reference Validator

A simple validator that accepts all non-empty messages:

```bash
cargo build --example reference_validator --release -p sui-dynamic-rpc-validator
```

### Parsing Validator

A validator that accepts messages only if they can be successfully parsed according to their expected format (protobuf or BCS):

```bash
cargo build --example parsing_validator --release -p sui-dynamic-rpc-validator --features parsing
```

The resulting shared objects will be located at:
- Linux: `target/release/examples/lib<name>.so`
- macOS: `target/release/examples/lib<name>.dylib`
- Windows: `target/release/examples/<name>.dll`

Where `<name>` is either `reference_validator` or `parsing_validator`.

## Creating Custom Validators

To create your own validator, implement the following C FFI functions:

### Required Functions

Each RPC method requires a corresponding validation function:

```rust
#[unsafe(no_mangle)]
pub extern "C" fn validate_submit_transaction(message_ptr: *const u8, message_len: usize) -> u8 {
    // Return 1 to accept, 0 to reject
}

#[unsafe(no_mangle)]
pub extern "C" fn validate_wait_for_effects(message_ptr: *const u8, message_len: usize) -> u8 {
    // Return 1 to accept, 0 to reject
}

// ... implement for other RPC methods
```

### Supported RPC Methods

The following RPC methods can have validation functions:
- `validate_submit_transaction`
- `validate_wait_for_effects`
- `validate_object_info`
- `validate_transaction_info`
- `validate_checkpoint`
- `validate_system_state`
- `validate_validator_health`

### Optional: Fast Path

Optionally implement a `should_validate` function to enable/disable validation:

```rust
#[unsafe(no_mangle)]
pub extern "C" fn should_validate() -> u8 {
    // Return 1 to enable validation, 0 to skip all validation (fast path)
}
```

## Configuration

Configure the validator in your node configuration:

```yaml
dynamic-rpc-validator-config:
  library-path: /path/to/validator.dylib
  check-interval-secs: 60
```

## Failure Handling

The system is designed to be extremely failure-tolerant:
- If the library cannot be loaded, all messages are accepted
- If a validation function panics, the message is accepted
- If a validation function is not found, the message is accepted
- File permission errors are handled gracefully

## Safety Requirements

**IMPORTANT:** Validation functions must:
- Not block indefinitely (no timeout protection is provided)
- Be thread-safe (may be called concurrently)
- Handle null pointers safely
- Not access memory outside the provided buffer

## Metrics

The following Prometheus metrics are exposed:
- `dynamic_validator_success` - Successful validations by RPC type
- `dynamic_validator_rejected` - Rejected validations by RPC type
- `dynamic_validator_errors` - Validation errors by type
- `dynamic_validator_load_attempts` - Library load attempts
- `dynamic_validator_load_success` - Successful loads
- `dynamic_validator_load_failures` - Load failures by type
- `dynamic_validator_fast_path_skips` - Fast path usage count
- `dynamic_validator_latency` - Validation latency by RPC type

## Testing

Run tests:

```bash
cargo test -p sui-dynamic-rpc-validator
```

## License

Apache-2.0
