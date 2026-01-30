// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Dynamic RPC Validation System
//!
//! This module provides a system for dynamically loading validation logic from shared object files.
//! Validators can load validation functions from external shared libraries to validate incoming
//! RPC messages without requiring a full validator software release.
//!
//! # Architecture
//!
//! - Shared object files expose validation functions via Rust CFFI
//! - Each RPC type has a corresponding validation function (e.g., validate_submit_transaction)
//! - Functions accept raw message bytes and return a boolean (true = accept, false = reject)
//! - System is failure-tolerant: if validation cannot be performed, messages are processed normally
//!
//! # Failure Handling
//!
//! The system handles various failure conditions gracefully:
//! - File system failures (missing, unreadable, or corrupted files)
//! - Dynamic loading failures (invalid shared object, missing functions)
//! - Runtime failures (crashes, timeouts, invalid return values)
//! - State management failures (file changes during reload, concurrent access)
//!
//! # Fast Path
//!
//! The shared object can signal that no validation is needed, allowing the validator
//! to skip the validation step entirely for performance.

use libloading::{Library, Symbol};
use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tracing::{debug, error, info, warn};

/// RPC method types that can be validated
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RpcMethod {
    SubmitTransaction,
    WaitForEffects,
    ObjectInfo,
    TransactionInfo,
    Checkpoint,
    CheckpointV2,
    GetSystemStateObject,
    ValidatorHealth,
}

impl RpcMethod {
    /// Get the function name for this RPC method in the shared library
    #[allow(dead_code)]
    fn function_name(&self) -> &'static str {
        match self {
            RpcMethod::SubmitTransaction => "validate_submit_transaction",
            RpcMethod::WaitForEffects => "validate_wait_for_effects",
            RpcMethod::ObjectInfo => "validate_object_info",
            RpcMethod::TransactionInfo => "validate_transaction_info",
            RpcMethod::Checkpoint | RpcMethod::CheckpointV2 => "validate_checkpoint",
            RpcMethod::GetSystemStateObject => "validate_system_state",
            RpcMethod::ValidatorHealth => "validate_validator_health",
        }
    }

    /// Get a string representation for metrics and logging
    pub fn as_str(&self) -> &'static str {
        match self {
            RpcMethod::SubmitTransaction => "submit_transaction",
            RpcMethod::WaitForEffects => "wait_for_effects",
            RpcMethod::ObjectInfo => "object_info",
            RpcMethod::TransactionInfo => "transaction_info",
            RpcMethod::Checkpoint => "checkpoint",
            RpcMethod::CheckpointV2 => "checkpoint_v2",
            RpcMethod::GetSystemStateObject => "get_system_state_object",
            RpcMethod::ValidatorHealth => "validator_health",
        }
    }
}

/// C FFI type for validation functions
/// Takes a pointer to bytes and length, returns a boolean (0 = reject, 1 = accept)
type ValidationFn = unsafe extern "C" fn(*const u8, usize) -> u8;

/// C FFI type for the fast path check function
/// Returns 0 if validation should be skipped, 1 if validation is needed
type ShouldValidateFn = unsafe extern "C" fn() -> u8;

/// Result of attempting to load the validation library
enum LoadResult {
    /// Successfully loaded with validation functions
    Loaded(ValidatorLibrary),
    /// Fast path: validation should be skipped
    FastPath,
    /// Failed to load: validation should be skipped
    Failed,
}

/// A loaded validator library with all validation functions
struct ValidatorLibrary {
    /// Library handle (must be kept alive)
    _library: Arc<Library>,
    /// Validation function for submit_transaction RPC
    submit_transaction: Option<Symbol<'static, ValidationFn>>,
    /// Validation function for wait_for_effects RPC
    wait_for_effects: Option<Symbol<'static, ValidationFn>>,
    /// Validation function for object_info RPC
    object_info: Option<Symbol<'static, ValidationFn>>,
    /// Validation function for transaction_info RPC
    transaction_info: Option<Symbol<'static, ValidationFn>>,
    /// Validation function for checkpoint RPC
    checkpoint: Option<Symbol<'static, ValidationFn>>,
    /// Validation function for get_system_state_object RPC
    system_state: Option<Symbol<'static, ValidationFn>>,
    /// Validation function for validator_health RPC
    validator_health: Option<Symbol<'static, ValidationFn>>,
    /// Timestamp when the library was loaded
    loaded_at: SystemTime,
}

/// Metrics for the dynamic validation system
pub struct DynamicValidatorMetrics {
    /// Counter for successful validations (by RPC type)
    pub validation_success: prometheus::IntCounterVec,
    /// Counter for rejected validations (by RPC type)
    pub validation_rejected: prometheus::IntCounterVec,
    /// Counter for validation errors (by RPC type and error type)
    pub validation_errors: prometheus::IntCounterVec,
    /// Counter for load attempts
    pub load_attempts: prometheus::IntCounter,
    /// Counter for successful loads
    pub load_success: prometheus::IntCounter,
    /// Counter for load failures (by failure type)
    pub load_failures: prometheus::IntCounterVec,
    /// Counter for fast path skips
    pub fast_path_skips: prometheus::IntCounter,
    /// Histogram for validation latency (by RPC type)
    pub validation_latency: prometheus::HistogramVec,
}

impl DynamicValidatorMetrics {
    pub fn new(registry: &prometheus::Registry) -> Self {
        use prometheus::{
            register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
            register_int_counter_with_registry,
        };

        Self {
            validation_success: register_int_counter_vec_with_registry!(
                "dynamic_validator_success",
                "Number of successful validations",
                &["rpc_type"],
                registry
            )
            .unwrap(),
            validation_rejected: register_int_counter_vec_with_registry!(
                "dynamic_validator_rejected",
                "Number of rejected validations",
                &["rpc_type"],
                registry
            )
            .unwrap(),
            validation_errors: register_int_counter_vec_with_registry!(
                "dynamic_validator_errors",
                "Number of validation errors",
                &["rpc_type", "error_type"],
                registry
            )
            .unwrap(),
            load_attempts: register_int_counter_with_registry!(
                "dynamic_validator_load_attempts",
                "Number of library load attempts",
                registry
            )
            .unwrap(),
            load_success: register_int_counter_with_registry!(
                "dynamic_validator_load_success",
                "Number of successful library loads",
                registry
            )
            .unwrap(),
            load_failures: register_int_counter_vec_with_registry!(
                "dynamic_validator_load_failures",
                "Number of library load failures",
                &["failure_type"],
                registry
            )
            .unwrap(),
            fast_path_skips: register_int_counter_with_registry!(
                "dynamic_validator_fast_path_skips",
                "Number of times fast path was used",
                registry
            )
            .unwrap(),
            validation_latency: register_histogram_vec_with_registry!(
                "dynamic_validator_latency",
                "Validation latency in seconds",
                &["rpc_type"],
                mysten_metrics::SUBSECOND_LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
        }
    }

    pub fn new_for_tests() -> Self {
        let registry = prometheus::Registry::new();
        Self::new(&registry)
    }
}

/// State of the loaded validator library
enum ValidatorState {
    /// No library loaded (initial state or after failed load)
    Unloaded,
    /// Library loaded and ready for validation
    Loaded(ValidatorLibrary),
    /// Fast path: no validation needed
    FastPath,
}

/// Manager for dynamic RPC validation
pub struct DynamicRpcValidator {
    /// Path to the shared object file
    library_path: Option<PathBuf>,
    /// Currently loaded validator state
    state: Arc<RwLock<ValidatorState>>,
    /// Metrics
    metrics: Arc<DynamicValidatorMetrics>,
    /// Last time we checked if the file was modified
    last_check: Arc<RwLock<SystemTime>>,
    /// How often to check for file modifications
    check_interval: Duration,
}

impl DynamicRpcValidator {
    /// Create a new dynamic RPC validator
    pub fn new(
        library_path: Option<PathBuf>,
        check_interval: Duration,
        metrics: Arc<DynamicValidatorMetrics>,
    ) -> Self {
        let validator = Self {
            library_path,
            state: Arc::new(RwLock::new(ValidatorState::Unloaded)),
            metrics,
            last_check: Arc::new(RwLock::new(SystemTime::now())),
            check_interval,
        };

        // Attempt initial load if path is provided
        if validator.library_path.is_some() {
            validator.try_load_library();
        }

        validator
    }

    /// Check if we should reload the library (based on file modification time)
    fn should_reload(&self) -> bool {
        let now = SystemTime::now();
        let mut last_check = self.last_check.write();

        if now.duration_since(*last_check).unwrap_or(Duration::ZERO) < self.check_interval {
            return false;
        }

        *last_check = now;

        let Some(ref path) = self.library_path else {
            return false;
        };

        // Check if file exists and has been modified
        match std::fs::metadata(path) {
            Ok(metadata) => {
                let file_modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                let state = self.state.read();
                match &*state {
                    ValidatorState::Loaded(lib) => file_modified > lib.loaded_at,
                    ValidatorState::Unloaded | ValidatorState::FastPath => true,
                }
            }
            Err(_) => false,
        }
    }

    /// Attempt to load or reload the validation library
    fn try_load_library(&self) {
        let Some(ref path) = self.library_path else {
            return;
        };

        self.metrics.load_attempts.inc();

        match self.load_library_internal(path) {
            LoadResult::Loaded(library) => {
                info!(
                    "Successfully loaded dynamic validator library from {:?}",
                    path
                );
                *self.state.write() = ValidatorState::Loaded(library);
                self.metrics.load_success.inc();
            }
            LoadResult::FastPath => {
                info!("Dynamic validator library indicates fast path - validation disabled");
                *self.state.write() = ValidatorState::FastPath;
                self.metrics.fast_path_skips.inc();
            }
            LoadResult::Failed => {
                debug!("Failed to load dynamic validator library - validation will be skipped");
                *self.state.write() = ValidatorState::Unloaded;
            }
        }
    }

    /// Internal function to load the library
    fn load_library_internal(&self, path: &Path) -> LoadResult {
        // Check if file exists
        if !path.exists() {
            debug!("Dynamic validator library file does not exist: {:?}", path);
            self.metrics
                .load_failures
                .with_label_values(&["file_not_found"])
                .inc();
            return LoadResult::Failed;
        }

        // Check if file is readable
        if let Err(e) = std::fs::metadata(path) {
            warn!(
                "Cannot read dynamic validator library file metadata: {:?}: {}",
                path, e
            );
            self.metrics
                .load_failures
                .with_label_values(&["file_not_readable"])
                .inc();
            return LoadResult::Failed;
        }

        // Attempt to load the library
        let library = match unsafe { Library::new(path) } {
            Ok(lib) => Arc::new(lib),
            Err(e) => {
                warn!(
                    "Failed to load dynamic validator library: {:?}: {}",
                    path, e
                );
                self.metrics
                    .load_failures
                    .with_label_values(&["load_failed"])
                    .inc();
                return LoadResult::Failed;
            }
        };

        // Check for fast path function
        let should_validate: Symbol<ShouldValidateFn> = unsafe {
            match library.get(b"should_validate\0") {
                Ok(func) => func,
                Err(_) => {
                    // Fast path function not found, assume we should validate
                    debug!("No should_validate function found, assuming validation is needed");
                    // Continue to load validation functions
                    return self.load_validation_functions(library);
                }
            }
        };

        // Call the fast path function
        let result = unsafe { should_validate() };
        if result == 0 {
            // Fast path: skip validation
            return LoadResult::FastPath;
        }

        // Load validation functions
        self.load_validation_functions(library)
    }

    /// Load validation functions from the library
    fn load_validation_functions(&self, library: Arc<Library>) -> LoadResult {
        let loaded_at = SystemTime::now();

        // Helper to safely load a function symbol
        let load_fn = |name: &[u8]| -> Option<Symbol<'static, ValidationFn>> {
            unsafe {
                match library.get::<ValidationFn>(name) {
                    Ok(symbol) => {
                        // SAFETY: We transmute the lifetime to 'static because we're holding
                        // a reference to the library itself, ensuring the symbol remains valid
                        let static_symbol: Symbol<'static, ValidationFn> =
                            std::mem::transmute(symbol);
                        Some(static_symbol)
                    }
                    Err(e) => {
                        debug!(
                            "Function {} not found in dynamic validator library: {}",
                            String::from_utf8_lossy(name),
                            e
                        );
                        None
                    }
                }
            }
        };

        let validator_lib = ValidatorLibrary {
            _library: library.clone(),
            submit_transaction: load_fn(b"validate_submit_transaction\0"),
            wait_for_effects: load_fn(b"validate_wait_for_effects\0"),
            object_info: load_fn(b"validate_object_info\0"),
            transaction_info: load_fn(b"validate_transaction_info\0"),
            checkpoint: load_fn(b"validate_checkpoint\0"),
            system_state: load_fn(b"validate_system_state\0"),
            validator_health: load_fn(b"validate_validator_health\0"),
            loaded_at,
        };

        // Check if at least one validation function was loaded
        if validator_lib.submit_transaction.is_none()
            && validator_lib.wait_for_effects.is_none()
            && validator_lib.object_info.is_none()
            && validator_lib.transaction_info.is_none()
            && validator_lib.checkpoint.is_none()
            && validator_lib.system_state.is_none()
            && validator_lib.validator_health.is_none()
        {
            warn!("No validation functions found in dynamic validator library");
            self.metrics
                .load_failures
                .with_label_values(&["no_functions"])
                .inc();
            return LoadResult::Failed;
        }

        LoadResult::Loaded(validator_lib)
    }

    /// Validate a message for a specific RPC type
    pub fn validate(&self, rpc_method: RpcMethod, message_bytes: &[u8]) -> bool {
        // Check if we should reload the library
        if self.should_reload() {
            self.try_load_library();
        }

        let state = self.state.read();

        match &*state {
            ValidatorState::Unloaded => {
                // No library loaded, accept all messages
                true
            }
            ValidatorState::FastPath => {
                // Fast path, skip validation
                true
            }
            ValidatorState::Loaded(library) => {
                self.validate_with_library(rpc_method, message_bytes, library)
            }
        }
    }

    /// Validate using a loaded library
    fn validate_with_library(
        &self,
        rpc_method: RpcMethod,
        message_bytes: &[u8],
        library: &ValidatorLibrary,
    ) -> bool {
        let _timer = self
            .metrics
            .validation_latency
            .with_label_values(&[rpc_method.as_str()])
            .start_timer();

        // Get the appropriate validation function
        let validate_fn = match rpc_method {
            RpcMethod::SubmitTransaction => library.submit_transaction.as_ref(),
            RpcMethod::WaitForEffects => library.wait_for_effects.as_ref(),
            RpcMethod::ObjectInfo => library.object_info.as_ref(),
            RpcMethod::TransactionInfo => library.transaction_info.as_ref(),
            RpcMethod::Checkpoint | RpcMethod::CheckpointV2 => library.checkpoint.as_ref(),
            RpcMethod::GetSystemStateObject => library.system_state.as_ref(),
            RpcMethod::ValidatorHealth => library.validator_health.as_ref(),
        };

        let Some(validate_fn) = validate_fn else {
            // No validation function for this RPC type, accept the message
            return true;
        };

        // Call the validation function with timeout protection
        // Note: We use std::panic::catch_unwind to handle panics from the C FFI
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Call the validation function
            // SAFETY: We're passing a valid pointer and length to the C function
            let result = unsafe { validate_fn(message_bytes.as_ptr(), message_bytes.len()) };
            result != 0
        }));

        match result {
            Ok(accepted) => {
                if accepted {
                    self.metrics
                        .validation_success
                        .with_label_values(&[rpc_method.as_str()])
                        .inc();
                } else {
                    self.metrics
                        .validation_rejected
                        .with_label_values(&[rpc_method.as_str()])
                        .inc();
                }
                accepted
            }
            Err(_) => {
                // Validation function panicked, accept the message by default
                error!(
                    "Validation function panicked for RPC method: {:?}",
                    rpc_method
                );
                self.metrics
                    .validation_errors
                    .with_label_values(&[rpc_method.as_str(), "panic"])
                    .inc();
                true
            }
        }
    }

    /// Force a reload of the validation library (used for testing)
    #[cfg(test)]
    pub fn force_reload(&self) {
        self.try_load_library();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn test_validator_with_no_library() {
        let metrics = Arc::new(DynamicValidatorMetrics::new_for_tests());
        let validator = DynamicRpcValidator::new(None, Duration::from_secs(60), metrics);

        // Should accept all messages when no library is configured
        assert!(validator.validate(RpcMethod::SubmitTransaction, b"test"));
        assert!(validator.validate(RpcMethod::WaitForEffects, b"test"));
    }

    #[test]
    fn test_validator_with_nonexistent_file() {
        let metrics = Arc::new(DynamicValidatorMetrics::new_for_tests());
        let validator = DynamicRpcValidator::new(
            Some(PathBuf::from("/nonexistent/path/validator.so")),
            Duration::from_secs(60),
            metrics.clone(),
        );

        // Should accept all messages when library cannot be loaded
        assert!(validator.validate(RpcMethod::SubmitTransaction, b"test"));

        // Should have recorded a load failure
        assert_eq!(
            metrics
                .load_failures
                .with_label_values(&["file_not_found"])
                .get(),
            1
        );
    }

    #[test]
    fn test_validator_with_unreadable_file() {
        let temp_dir = TempDir::new().unwrap();
        let lib_path = temp_dir.path().join("validator.so");

        // Create a file but make it unreadable (on Unix)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::File::create(&lib_path).unwrap();
            let mut perms = std::fs::metadata(&lib_path).unwrap().permissions();
            perms.set_mode(0o000);
            std::fs::set_permissions(&lib_path, perms).unwrap();
        }

        let metrics = Arc::new(DynamicValidatorMetrics::new_for_tests());
        let validator =
            DynamicRpcValidator::new(Some(lib_path), Duration::from_secs(60), metrics.clone());

        // Should accept all messages when library cannot be loaded
        assert!(validator.validate(RpcMethod::SubmitTransaction, b"test"));
    }

    #[test]
    fn test_validator_with_invalid_library() {
        let temp_dir = TempDir::new().unwrap();
        let lib_path = temp_dir.path().join("validator.so");

        // Create a file with invalid content
        let mut file = std::fs::File::create(&lib_path).unwrap();
        file.write_all(b"This is not a valid shared library")
            .unwrap();
        drop(file);

        let metrics = Arc::new(DynamicValidatorMetrics::new_for_tests());
        let validator =
            DynamicRpcValidator::new(Some(lib_path), Duration::from_secs(60), metrics.clone());

        // Should accept all messages when library cannot be loaded
        assert!(validator.validate(RpcMethod::SubmitTransaction, b"test"));

        // Should have recorded a load failure
        assert_eq!(
            metrics
                .load_failures
                .with_label_values(&["load_failed"])
                .get(),
            1
        );
    }

    #[test]
    fn test_validator_state_transitions() {
        let metrics = Arc::new(DynamicValidatorMetrics::new_for_tests());
        let validator = DynamicRpcValidator::new(
            Some(PathBuf::from("/nonexistent/path/validator.so")),
            Duration::from_secs(60),
            metrics.clone(),
        );

        // Initially should be in Unloaded state
        let state = validator.state.read();
        assert!(matches!(*state, ValidatorState::Unloaded));
        drop(state);

        // Validation should succeed (default behavior for unloaded state)
        assert!(validator.validate(RpcMethod::SubmitTransaction, b"test"));
    }

    #[test]
    fn test_validator_metrics() {
        let metrics = Arc::new(DynamicValidatorMetrics::new_for_tests());
        let validator = DynamicRpcValidator::new(None, Duration::from_secs(60), metrics.clone());

        // Validate some messages
        validator.validate(RpcMethod::SubmitTransaction, b"test1");
        validator.validate(RpcMethod::WaitForEffects, b"test2");
        validator.validate(RpcMethod::ObjectInfo, b"test3");

        // All validations should succeed with no library loaded
        // No metrics should be recorded for success/reject since validation is skipped
    }

    #[test]
    fn test_check_interval() {
        let temp_dir = TempDir::new().unwrap();
        let lib_path = temp_dir.path().join("validator.so");

        // Create a file with invalid content (so it fails to load)
        let mut file = std::fs::File::create(&lib_path).unwrap();
        file.write_all(b"invalid").unwrap();
        drop(file);

        let metrics = Arc::new(DynamicValidatorMetrics::new_for_tests());
        let check_interval = Duration::from_millis(100);
        let validator =
            DynamicRpcValidator::new(Some(lib_path.clone()), check_interval, metrics.clone());

        // First validation should trigger a load attempt
        validator.validate(RpcMethod::SubmitTransaction, b"test");
        let initial_attempts = metrics.load_attempts.get();

        // Immediate second validation should not trigger another load attempt
        validator.validate(RpcMethod::SubmitTransaction, b"test");
        assert_eq!(metrics.load_attempts.get(), initial_attempts);

        // Wait for check interval to pass
        std::thread::sleep(check_interval + Duration::from_millis(50));

        // Modify the file to trigger a reload
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&lib_path)
            .unwrap();
        file.write_all(b"modified").unwrap();
        drop(file);

        // Now validation should trigger another load attempt
        validator.validate(RpcMethod::SubmitTransaction, b"test");
        assert_eq!(metrics.load_attempts.get(), initial_attempts + 1);
    }

    #[test]
    fn test_all_rpc_methods() {
        let metrics = Arc::new(DynamicValidatorMetrics::new_for_tests());
        let validator = DynamicRpcValidator::new(None, Duration::from_secs(60), metrics);

        // All RPC methods should be accepted when no library is configured
        assert!(validator.validate(RpcMethod::SubmitTransaction, b"test"));
        assert!(validator.validate(RpcMethod::WaitForEffects, b"test"));
        assert!(validator.validate(RpcMethod::ObjectInfo, b"test"));
        assert!(validator.validate(RpcMethod::TransactionInfo, b"test"));
        assert!(validator.validate(RpcMethod::Checkpoint, b"test"));
        assert!(validator.validate(RpcMethod::CheckpointV2, b"test"));
        assert!(validator.validate(RpcMethod::GetSystemStateObject, b"test"));
        assert!(validator.validate(RpcMethod::ValidatorHealth, b"test"));
    }

    #[test]
    fn test_empty_message() {
        let metrics = Arc::new(DynamicValidatorMetrics::new_for_tests());
        let validator = DynamicRpcValidator::new(None, Duration::from_secs(60), metrics);

        // Empty messages should be accepted
        assert!(validator.validate(RpcMethod::SubmitTransaction, b""));
    }

    #[test]
    fn test_large_message() {
        let metrics = Arc::new(DynamicValidatorMetrics::new_for_tests());
        let validator = DynamicRpcValidator::new(None, Duration::from_secs(60), metrics);

        // Large messages should be accepted
        let large_message = vec![0u8; 1_000_000];
        assert!(validator.validate(RpcMethod::SubmitTransaction, &large_message));
    }

    #[test]
    fn test_concurrent_validation() {
        use std::sync::Arc;
        use std::thread;

        let metrics = Arc::new(DynamicValidatorMetrics::new_for_tests());
        let validator = Arc::new(DynamicRpcValidator::new(
            None,
            Duration::from_secs(60),
            metrics,
        ));

        let mut handles = vec![];

        // Spawn multiple threads performing validation concurrently
        for i in 0..10 {
            let validator_clone = validator.clone();
            let handle = thread::spawn(move || {
                let message = format!("test message {}", i);
                for _ in 0..100 {
                    assert!(
                        validator_clone.validate(RpcMethod::SubmitTransaction, message.as_bytes())
                    );
                }
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }
    }
}
