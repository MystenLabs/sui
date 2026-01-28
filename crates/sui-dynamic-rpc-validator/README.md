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

### Reference Implementation

A reference validator implementation is included as an example that can be compiled to a shared object.

## Building the Reference Implementation

To build the reference validator as a shared object:

```bash
cargo build --example reference_validator --release -p sui-dynamic-rpc-validator
```

The resulting shared object will be located at:
- Linux: `target/release/examples/libreference_validator.so`
- macOS: `target/release/examples/libreference_validator.dylib`
- Windows: `target/release/examples/reference_validator.dll`

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
