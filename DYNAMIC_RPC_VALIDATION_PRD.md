# Product Requirements Document: Dynamic RPC Validation System

## 1. Overview
System to allow validators to load validation logic from shared object files, enabling rapid deployment of validation checks without full validator releases.

The system addresses the need to quickly ship new validation checks for raw messages received over the validator's RPC interface without requiring a full release of the validator software (sui-node). Validators will dynamically load validation logic from shared object files and apply these checks to incoming RPC messages, allowing validation rules to be updated independently of the validator binary.

## 2. Core Requirements

### 2.1. Architecture
- 2.1.1. Shared object file contains validation functions exposed via Rust CFFI
- 2.1.2. Functions accept raw RPC messages as input
- 2.1.3. Functions return Boolean (true = accept, false = reject)
- 2.1.4. Each RPC type has corresponding validation function
  - Example: `validate_submit_transaction` for submit transaction RPC
- 2.1.5. Validators dynamically load shared object and function pointers at runtime

### 2.2. Loading Behavior
- 2.2.1. Validator periodically checks for presence of shared object file
- 2.2.2. If file present, dynamically load it
- 2.2.3. Load function pointers for each RPC validation function
- 2.2.4. Periodically reload shared object if file changes

### 2.3. Validation Execution
- 2.3.1. Call corresponding validation function for each incoming RPC message
- 2.3.2. Use function's Boolean return value to accept or reject message

### 2.4. Fast Path / Happy Path
- 2.4.1. Shared object can declare to validator that no additional validation is needed
- 2.4.2. When declared, validator skips validation step entirely

### 2.5. Failure Handling
- 2.5.1. System must be extremely failure-tolerant
- 2.5.2. If shared object cannot be loaded, validator processes all messages normally (skip validation)

## 3. Failure Conditions to Handle

### 3.1. File System Failures
- 3.1.1. Shared object file does not exist
- 3.1.2. Shared object file exists but cannot be read (permissions)
- 3.1.3. Shared object file is corrupted or invalid format

### 3.2. Dynamic Loading Failures
- 3.2.1. Shared object cannot be dynamically loaded
- 3.2.2. Required validation functions not found in shared object
- 3.2.3. Function signatures do not match expected CFFI interface

### 3.3. Runtime Failures
- 3.3.1. Validation function crashes or panics
- 3.3.2. Validation function times out or hangs
- 3.3.3. Validation function returns invalid data

### 3.4. State Management Failures
- 3.4.1. File changes during reload operation
- 3.4.2. File deleted while validator is using it
- 3.4.3. Concurrent access issues during reload
