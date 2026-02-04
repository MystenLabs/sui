// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    IndexKind,
    file_format::{CodeOffset, FunctionDefinitionIndex, TableIndex},
};
use move_core_types::{
    account_address::AccountAddress,
    language_storage::ModuleId,
    vm_status::{StatusCode, StatusType},
};
use std::fmt;

// Controls whether to capture backtraces on error construction in debug builds.
// We don't want to do this unconditionally in debug builds even if `RUST_BACKTRACE` is set
// because we may also (and do) use the debug format of the VM errors in the expected values.
//
// Instead we condition the backtrace capture on the presence of a dedicated env var along with
// `RUST_BACKTRACE` being set.
#[cfg(debug_assertions)]
static BACKTRACE_ON_ERROR: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

#[cfg(debug_assertions)]
fn backtrace_on_error() -> bool {
    *BACKTRACE_ON_ERROR.get_or_init(|| std::env::var("MOVE_VM_ERROR_LOCATION").is_ok())
}

pub type VMResult<T> = ::std::result::Result<T, VMError>;
pub type BinaryLoaderResult<T> = ::std::result::Result<T, PartialVMError>;
pub type PartialVMResult<T> = ::std::result::Result<T, PartialVMError>;
pub type HCFResult<T> = ::std::result::Result<T, HCFError>;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Location {
    Undefined,
    // The `AccountAddress` inside of the `Module`'s `ModuleId` is the original id
    Module(ModuleId),
    // The `AccountAddress` inside of the `Package` is the version id of the package
    Package(AccountAddress),
}

/// A representation of the execution state (e.g., stack trace) at an
/// error point.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ExecutionState {
    stack_trace: Vec<(ModuleId, FunctionDefinitionIndex, CodeOffset)>,
    // we may consider adding more state if necessary
}

impl ExecutionState {
    pub fn new(stack_trace: Vec<(ModuleId, FunctionDefinitionIndex, CodeOffset)>) -> Self {
        Self { stack_trace }
    }

    pub fn stack_trace(&self) -> &Vec<(ModuleId, FunctionDefinitionIndex, CodeOffset)> {
        &self.stack_trace
    }
}

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
pub struct VMError(Box<VMError_>);

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
struct VMError_ {
    major_status: StatusCode,
    sub_status: Option<u64>,
    message: Option<String>,
    exec_state: Option<ExecutionState>,
    location: Location,
    indices: Vec<(IndexKind, TableIndex)>,
    offsets: Vec<(FunctionDefinitionIndex, CodeOffset)>,
    #[cfg(debug_assertions)]
    backtrace: Option<String>,
}

impl VMError {
    pub fn major_status(&self) -> StatusCode {
        self.0.major_status
    }

    pub fn sub_status(&self) -> Option<u64> {
        self.0.sub_status
    }

    pub fn message(&self) -> Option<&String> {
        self.0.message.as_ref()
    }

    pub fn exec_state(&self) -> Option<&ExecutionState> {
        self.0.exec_state.as_ref()
    }

    pub fn remove_exec_state(&mut self) {
        self.0.exec_state = None;
    }

    pub fn location(&self) -> &Location {
        &self.0.location
    }

    pub fn indices(&self) -> &Vec<(IndexKind, TableIndex)> {
        &self.0.indices
    }

    pub fn offsets(&self) -> &Vec<(FunctionDefinitionIndex, CodeOffset)> {
        &self.0.offsets
    }

    pub fn status_type(&self) -> StatusType {
        self.0.major_status.status_type()
    }

    #[allow(clippy::type_complexity)]
    pub fn all_data(
        self,
    ) -> (
        StatusCode,
        Option<u64>,
        Option<String>,
        Option<ExecutionState>,
        Location,
        Vec<(IndexKind, TableIndex)>,
        Vec<(FunctionDefinitionIndex, CodeOffset)>,
    ) {
        let VMError_ {
            major_status,
            sub_status,
            message,
            exec_state,
            location,
            indices,
            offsets,
            #[cfg(debug_assertions)]
                backtrace: _,
        } = *self.0;
        (
            major_status,
            sub_status,
            message,
            exec_state,
            location,
            indices,
            offsets,
        )
    }

    pub fn to_partial(self) -> PartialVMError {
        let VMError_ {
            major_status,
            sub_status,
            message,
            exec_state,
            indices,
            offsets,
            #[cfg(debug_assertions)]
            backtrace,
            ..
        } = *self.0;
        PartialVMError_ {
            major_status,
            sub_status,
            message,
            exec_state,
            indices,
            offsets,
            #[cfg(debug_assertions)]
            backtrace,
        }
        .into()
    }
}

impl fmt::Debug for VMError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Debug for VMError_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            major_status,
            sub_status,
            message,
            exec_state,
            location,
            indices,
            offsets,
            #[cfg(debug_assertions)]
            backtrace,
        } = self;
        f.debug_struct("VMError")
            .field("major_status", major_status)
            .field("sub_status", sub_status)
            .field("message", message)
            .field("exec_state", exec_state)
            .field("location", location)
            .field("indices", indices)
            .field("offsets", offsets)
            .finish()?;

        #[cfg(debug_assertions)]
        if let Some(backtrace) = backtrace {
            writeln!(f, "\nError construction location backtrace:")?;
            writeln!(f, "{}", backtrace)?;
        }

        Ok(())
    }
}

impl std::error::Error for VMError {}

#[derive(Clone)]
pub enum PartialVMError {
    HCF(Box<HCFError>),
    #[allow(private_interfaces)]
    Error(Box<PartialVMError_>),
}

#[derive(Clone)]
pub struct HCFError {
    pub message: Option<String>,
}

#[derive(Clone)]
struct PartialVMError_ {
    major_status: StatusCode,
    sub_status: Option<u64>,
    message: Option<String>,
    exec_state: Option<ExecutionState>,
    indices: Vec<(IndexKind, TableIndex)>,
    offsets: Vec<(FunctionDefinitionIndex, CodeOffset)>,
    #[cfg(debug_assertions)]
    backtrace: Option<String>,
}

impl PartialVMError {
    // TODO: This should return `Result<_, HCFError>`
    #[allow(clippy::type_complexity)]
    pub fn all_data(
        self,
    ) -> (
        StatusCode,
        Option<u64>,
        Option<String>,
        Option<ExecutionState>,
        Vec<(IndexKind, TableIndex)>,
        Vec<(FunctionDefinitionIndex, CodeOffset)>,
    ) {
        let (status, minor, msg, state, _loc, indicies, offsets) =
            self.finish(Location::Undefined).all_data();
        (status, minor, msg, state, indicies, offsets)
    }

    pub fn new(major_status: StatusCode) -> Self {
        debug_assert!(
            major_status != StatusCode::INVALID_MOVE_RUNTIME_ERROR,
            "Use MoveRuntimeError for runtime errors"
        );
        #[cfg(debug_assertions)]
        let backtrace = {
            if !backtrace_on_error() {
                None
            } else {
                let bt = std::backtrace::Backtrace::capture();
                if bt.status() == std::backtrace::BacktraceStatus::Captured {
                    Some(format!("{}", bt))
                } else {
                    None
                }
            }
        };
        PartialVMError_ {
            major_status,
            sub_status: None,
            message: None,
            exec_state: None,
            indices: vec![],
            offsets: vec![],
            #[cfg(debug_assertions)]
            backtrace,
        }
        .into()
    }
}

// -------------------------------------------------------------------------------------------------
// Error Impls
// -------------------------------------------------------------------------------------------------

// -----------------------------------------------
// Partial Error Trait and Operations
// -----------------------------------------------

macro_rules! impl_partial_vm_error {
    ($name:ident, Self $(, $arg:ident : $argty:ty )* $(,)?) => {
        pub fn $name(self $(, $arg : $argty )* ) -> Self {
            match self {
                PartialVMError::HCF(hcf) => PartialVMError::HCF(Box::new(hcf.$name($($arg),*))),
                PartialVMError::Error(err) => PartialVMError::Error(Box::new(err.$name($($arg),*))),
            }
        }
    };

    ($name:ident, $ret:ty $(, $arg:ident : $argty:ty )* $(,)?) => {
        pub fn $name(self $(, $arg : $argty )* ) -> $ret {
            match self {
                PartialVMError::HCF(hcf) => hcf.$name($($arg),*),
                PartialVMError::Error(err) => err.$name($($arg),*),
            }
        }
    };
}

impl PartialVMError {
    pub fn major_status(&self) -> StatusCode {
        match self {
            PartialVMError::HCF(hcf) => hcf.major_status(),
            PartialVMError::Error(err) => err.major_status(),
        }
    }

    impl_partial_vm_error!(with_sub_status, Self, sub_status: u64);
    impl_partial_vm_error!(with_message, Self, message: String);
    impl_partial_vm_error!(with_exec_state, Self, exec_state: ExecutionState);
    impl_partial_vm_error!(at_index, Self, kind: IndexKind, index: TableIndex);
    impl_partial_vm_error!(at_indices, Self, additional_indices: Vec<(IndexKind, TableIndex)>);
    impl_partial_vm_error!(at_code_offset, Self, function: FunctionDefinitionIndex, offset: CodeOffset);
    impl_partial_vm_error!(at_code_offsets, Self, additional_offsets: Vec<(FunctionDefinitionIndex, CodeOffset)>);
    impl_partial_vm_error!(append_message_with_separator, Self, separator: char, additional_message: String);
    impl_partial_vm_error!(finish, VMError, location: Location);
}

pub trait PartialVMErrorImpl {
    fn major_status(&self) -> StatusCode;
    fn with_sub_status(self, sub_status: u64) -> Self;
    fn with_message(self, message: String) -> Self;
    fn with_exec_state(self, exec_state: ExecutionState) -> Self;
    fn at_index(self, kind: IndexKind, index: TableIndex) -> Self;
    fn at_indices(self, additional_indices: Vec<(IndexKind, TableIndex)>) -> Self;
    fn at_code_offset(self, function: FunctionDefinitionIndex, offset: CodeOffset) -> Self;
    fn at_code_offsets(
        self,
        additional_offsets: Vec<(FunctionDefinitionIndex, CodeOffset)>,
    ) -> Self;
    fn append_message_with_separator(self, separator: char, additional_message: String) -> Self;
    fn finish(self, location: Location) -> VMError;
}

impl PartialVMErrorImpl for HCFError {
    fn major_status(&self) -> StatusCode {
        StatusCode::INVALID_MOVE_RUNTIME_ERROR
    }

    fn with_sub_status(self, _sub_status: u64) -> Self {
        self
    }

    fn with_message(mut self, message: String) -> Self {
        self.message = Some(message);
        self
    }

    fn with_exec_state(self, _exec_state: ExecutionState) -> Self {
        self
    }

    fn at_index(self, _kind: IndexKind, _index: TableIndex) -> Self {
        self
    }

    fn at_indices(self, _additional_indices: Vec<(IndexKind, TableIndex)>) -> Self {
        self
    }

    fn at_code_offset(self, _function: FunctionDefinitionIndex, _offset: CodeOffset) -> Self {
        self
    }

    fn at_code_offsets(
        self,
        _additional_offsets: Vec<(FunctionDefinitionIndex, CodeOffset)>,
    ) -> Self {
        self
    }

    fn append_message_with_separator(
        mut self,
        separator: char,
        additional_message: String,
    ) -> Self {
        match &mut self.message {
            Some(msg) => {
                if !msg.is_empty() {
                    msg.push(separator);
                }
                msg.push_str(&additional_message);
            }
            None => self.message = Some(additional_message),
        };
        self
    }

    fn finish(self, location: Location) -> VMError {
        let HCFError { message } = self;
        VMError(Box::new(VMError_ {
            major_status: StatusCode::INVALID_MOVE_RUNTIME_ERROR,
            sub_status: None,
            message,
            exec_state: None,
            location,
            indices: vec![],
            offsets: vec![],
            #[cfg(debug_assertions)]
            backtrace: None,
        }))
    }
}

impl PartialVMErrorImpl for PartialVMError_ {
    fn major_status(&self) -> StatusCode {
        self.major_status
    }

    fn with_sub_status(mut self, sub_status: u64) -> Self {
        debug_assert!(self.sub_status.is_none());
        self.sub_status = Some(sub_status);
        self
    }

    fn with_message(mut self, message: String) -> Self {
        debug_assert!(self.message.is_none());
        self.message = Some(message);
        self
    }

    fn with_exec_state(mut self, exec_state: ExecutionState) -> Self {
        debug_assert!(self.exec_state.is_none());
        self.exec_state = Some(exec_state);
        self
    }

    fn at_index(mut self, kind: IndexKind, index: TableIndex) -> Self {
        self.indices.push((kind, index));
        self
    }

    fn at_indices(mut self, additional_indices: Vec<(IndexKind, TableIndex)>) -> Self {
        self.indices.extend(additional_indices);
        self
    }

    fn at_code_offset(mut self, function: FunctionDefinitionIndex, offset: CodeOffset) -> Self {
        self.offsets.push((function, offset));
        self
    }

    fn at_code_offsets(
        mut self,
        additional_offsets: Vec<(FunctionDefinitionIndex, CodeOffset)>,
    ) -> Self {
        self.offsets.extend(additional_offsets);
        self
    }

    fn append_message_with_separator(
        mut self,
        separator: char,
        additional_message: String,
    ) -> Self {
        match &mut self.message {
            Some(msg) => {
                if !msg.is_empty() {
                    msg.push(separator);
                }
                msg.push_str(&additional_message);
            }
            None => self.message = Some(additional_message),
        };
        self
    }

    fn finish(self, location: Location) -> VMError {
        let PartialVMError_ {
            major_status,
            sub_status,
            message,
            exec_state,
            indices,
            offsets,
            #[cfg(debug_assertions)]
            backtrace,
        } = self;
        debug_assert!(
            major_status != StatusCode::INVALID_MOVE_RUNTIME_ERROR,
            "Use MoveRuntimeError for runtime errors"
        );
        VMError(Box::new(VMError_ {
            major_status,
            sub_status,
            message,
            exec_state,
            location,
            indices,
            offsets,
            #[cfg(debug_assertions)]
            backtrace,
        }))
    }
}

// -----------------------------------------------
// Specific Sub-Error Impls
// -----------------------------------------------

impl HCFError {
    pub fn new() -> Self {
        Self { message: None }
    }
}

// -----------------------------------------------
// Conversion Impls
// -----------------------------------------------

impl From<HCFError> for PartialVMError {
    fn from(hcf: HCFError) -> Self {
        PartialVMError::HCF(Box::new(hcf))
    }
}

impl From<PartialVMError_> for PartialVMError {
    fn from(err: PartialVMError_) -> Self {
        PartialVMError::Error(Box::new(err))
    }
}

impl From<HCFError> for VMError {
    fn from(panic: HCFError) -> Self {
        panic.finish(Location::Undefined)
    }
}

// -----------------------------------------------
// Display Impls
// -----------------------------------------------

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Location::Undefined => write!(f, "UNDEFINED"),
            Location::Module(id) => write!(f, "Module {:?}", id),
            Location::Package(addr) => write!(f, "Package {:?}", addr),
        }
    }
}

impl fmt::Display for HCFError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.message {
            Some(msg) => write!(f, "HCFError with message {}", msg),
            None => write!(f, "HCFError with no message"),
        }
    }
}

impl fmt::Display for PartialVMError_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut status = format!("PartialVMError with status {:#?}", self.major_status);

        if let Some(sub_status) = self.sub_status {
            status = format!("{} with sub status {}", status, sub_status);
        }

        if let Some(msg) = &self.message {
            status = format!("{} and message {}", status, msg);
        }

        for (kind, index) in &self.indices {
            status = format!("{} at index {} for {}", status, index, kind);
        }
        for (fdef, code_offset) in &self.offsets {
            status = format!(
                "{} at code offset {} in function definition {}",
                status, code_offset, fdef
            );
        }

        write!(f, "{}", status)
    }
}

impl fmt::Display for PartialVMError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PartialVMError::HCF(hcf) => fmt::Display::fmt(hcf, f),
            PartialVMError::Error(err) => fmt::Display::fmt(err, f),
        }
    }
}

impl fmt::Display for VMError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut status = format!("VMError with status {:#?}", self.0.major_status);

        if let Some(sub_status) = self.0.sub_status {
            status = format!("{} with sub status {}", status, sub_status);
        }

        status = format!("{} at location {}", status, self.0.location);

        if let Some(msg) = &self.0.message {
            status = format!("{} and message {}", status, msg);
        }

        for (kind, index) in &self.0.indices {
            status = format!("{} at index {} for {}", status, index, kind);
        }
        for (fdef, code_offset) in &self.0.offsets {
            status = format!(
                "{} at code offset {} in function definition {}",
                status, code_offset, fdef
            );
        }

        write!(f, "{}", status)
    }
}

////////////////////////////////////////////////////////////////////////////
// Conversion functions from internal VM statuses into external VM statuses
////////////////////////////////////////////////////////////////////////////

pub fn offset_out_of_bounds(
    status: StatusCode,
    kind: IndexKind,
    target_offset: usize,
    target_pool_len: usize,
    cur_function: FunctionDefinitionIndex,
    cur_bytecode_offset: CodeOffset,
) -> PartialVMError {
    let msg = format!(
        "Index {} out of bounds for {} at bytecode offset {} in function {} while indexing {}",
        target_offset, target_pool_len, cur_bytecode_offset, cur_function, kind
    );
    PartialVMError::new(status)
        .with_message(msg)
        .at_code_offset(cur_function, cur_bytecode_offset)
}

pub fn bounds_error(
    status: StatusCode,
    kind: IndexKind,
    idx: TableIndex,
    len: usize,
) -> PartialVMError {
    let msg = format!(
        "Index {} out of bounds for {} while indexing {}",
        idx, len, kind
    );
    PartialVMError::new(status)
        .at_index(kind, idx)
        .with_message(msg)
}

pub fn verification_error(status: StatusCode, kind: IndexKind, idx: TableIndex) -> PartialVMError {
    PartialVMError::new(status).at_index(kind, idx)
}

impl fmt::Debug for PartialVMError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PartialVMError::HCF(hcf_error) => std::fmt::Debug::fmt(&hcf_error, f),
            PartialVMError::Error(partial_vmerror) => std::fmt::Debug::fmt(&partial_vmerror, f),
        }
    }
}

impl fmt::Debug for HCFError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { message } = self;
        f.debug_struct("HCFError")
            .field("message", message)
            .finish()
    }
}

impl fmt::Debug for PartialVMError_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            major_status,
            sub_status,
            message,
            exec_state,
            indices,
            offsets,
            #[cfg(debug_assertions)]
            backtrace,
        } = self;
        f.debug_struct("PartialVMError")
            .field("major_status", major_status)
            .field("sub_status", sub_status)
            .field("message", message)
            .field("exec_state", exec_state)
            .field("indices", indices)
            .field("offsets", offsets)
            .finish()?;

        #[cfg(debug_assertions)]
        if let Some(backtrace) = backtrace {
            writeln!(f, "\nError construction location backtrace:")?;
            writeln!(f, "{}", backtrace)?;
        }

        Ok(())
    }
}

impl std::error::Error for PartialVMError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}
