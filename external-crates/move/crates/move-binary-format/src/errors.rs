// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    file_format::{CodeOffset, FunctionDefinitionIndex, TableIndex},
    IndexKind,
};
use move_core_types::{
    language_storage::ModuleId,
    vm_status::{self, StatusCode, StatusType, VMStatus},
};
use std::fmt;

pub type VMResult<T> = ::std::result::Result<T, VMError>;
pub type BinaryLoaderResult<T> = ::std::result::Result<T, PartialVMError>;
pub type PartialVMResult<T> = ::std::result::Result<T, PartialVMError>;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Location {
    Undefined,
    Script,
    Module(ModuleId),
}

/// A representation of the execution state (e.g., stack trace) at an
/// error point.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ExecutionState {
    stack_trace: Vec<(Option<ModuleId>, FunctionDefinitionIndex, CodeOffset)>,
    // we may consider adding more state if necessary
}

impl ExecutionState {
    pub fn new(stack_trace: Vec<(Option<ModuleId>, FunctionDefinitionIndex, CodeOffset)>) -> Self {
        Self { stack_trace }
    }

    pub fn stack_trace(&self) -> &Vec<(Option<ModuleId>, FunctionDefinitionIndex, CodeOffset)> {
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
}

impl VMError {
    pub fn into_vm_status(self) -> VMStatus {
        let VMError_ {
            major_status,
            sub_status,
            location,
            mut offsets,
            ..
        } = *self.0;
        match (major_status, sub_status, location) {
            (StatusCode::EXECUTED, sub_status, _) => {
                debug_assert!(sub_status.is_none());
                VMStatus::Executed
            }
            (StatusCode::ABORTED, Some(code), Location::Script) => {
                VMStatus::MoveAbort(vm_status::AbortLocation::Script, code)
            }
            (StatusCode::ABORTED, Some(code), Location::Module(id)) => {
                VMStatus::MoveAbort(vm_status::AbortLocation::Module(id), code)
            }

            (StatusCode::ABORTED, sub_status, location) => {
                debug_assert!(
                    false,
                    "Expected a code and module/script location with ABORTED, but got {:?} and {}",
                    sub_status, location
                );
                VMStatus::Error(StatusCode::ABORTED)
            }

            (major_status, sub_status, location)
                if major_status.status_type() == StatusType::Execution =>
            {
                let abort_location = match &location {
                    Location::Script => vm_status::AbortLocation::Script,
                    Location::Module(id) => vm_status::AbortLocation::Module(id.clone()),
                    Location::Undefined => {
                        return VMStatus::Error(major_status);
                    }
                };
                // Errors for OUT_OF_GAS do not always have index set: if it does not, it should already return above.
                debug_assert!(
                    offsets.len() == 1,
                    "Unexpected offsets. major_status: {:?}\
                    sub_status: {:?}\
                    location: {:?}\
                    offsets: {:#?}",
                    major_status,
                    sub_status,
                    location,
                    offsets
                );
                let (function, code_offset) = match offsets.pop() {
                    None => {
                        return VMStatus::Error(major_status);
                    }
                    Some((fdef_idx, code_offset)) => (fdef_idx.0, code_offset),
                };
                VMStatus::ExecutionFailure {
                    status_code: major_status,
                    location: abort_location,
                    function,
                    code_offset,
                }
            }

            (major_status, _, _) => VMStatus::Error(major_status),
        }
    }

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
            ..
        } = *self.0;
        PartialVMError(Box::new(PartialVMError_ {
            major_status,
            sub_status,
            message,
            exec_state,
            indices,
            offsets,
        }))
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
        } = self;
        f.debug_struct("VMError")
            .field("major_status", major_status)
            .field("sub_status", sub_status)
            .field("message", message)
            .field("exec_state", exec_state)
            .field("location", location)
            .field("indices", indices)
            .field("offsets", offsets)
            .finish()
    }
}

impl std::error::Error for VMError {}

#[derive(Clone)]
pub struct PartialVMError(Box<PartialVMError_>);

#[derive(Clone)]
struct PartialVMError_ {
    major_status: StatusCode,
    sub_status: Option<u64>,
    message: Option<String>,
    exec_state: Option<ExecutionState>,
    indices: Vec<(IndexKind, TableIndex)>,
    offsets: Vec<(FunctionDefinitionIndex, CodeOffset)>,
}

impl PartialVMError {
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
        let PartialVMError_ {
            major_status,
            sub_status,
            message,
            exec_state,
            indices,
            offsets,
        } = *self.0;
        (
            major_status,
            sub_status,
            message,
            exec_state,
            indices,
            offsets,
        )
    }

    pub fn finish(self, location: Location) -> VMError {
        let PartialVMError_ {
            major_status,
            sub_status,
            message,
            exec_state,
            indices,
            offsets,
        } = *self.0;
        VMError(Box::new(VMError_ {
            major_status,
            sub_status,
            message,
            exec_state,
            location,
            indices,
            offsets,
        }))
    }

    pub fn new(major_status: StatusCode) -> Self {
        Self(Box::new(PartialVMError_ {
            major_status,
            sub_status: None,
            message: None,
            exec_state: None,
            indices: vec![],
            offsets: vec![],
        }))
    }

    pub fn major_status(&self) -> StatusCode {
        self.0.major_status
    }

    pub fn with_sub_status(mut self, sub_status: u64) -> Self {
        debug_assert!(self.0.sub_status.is_none());
        self.0.sub_status = Some(sub_status);
        self
    }

    pub fn with_message(mut self, message: String) -> Self {
        debug_assert!(self.0.message.is_none());
        self.0.message = Some(message);
        self
    }

    pub fn with_exec_state(mut self, exec_state: ExecutionState) -> Self {
        debug_assert!(self.0.exec_state.is_none());
        self.0.exec_state = Some(exec_state);
        self
    }

    pub fn at_index(mut self, kind: IndexKind, index: TableIndex) -> Self {
        self.0.indices.push((kind, index));
        self
    }

    pub fn at_indices(mut self, additional_indices: Vec<(IndexKind, TableIndex)>) -> Self {
        self.0.indices.extend(additional_indices);
        self
    }

    pub fn at_code_offset(mut self, function: FunctionDefinitionIndex, offset: CodeOffset) -> Self {
        self.0.offsets.push((function, offset));
        self
    }

    pub fn at_code_offsets(
        mut self,
        additional_offsets: Vec<(FunctionDefinitionIndex, CodeOffset)>,
    ) -> Self {
        self.0.offsets.extend(additional_offsets);
        self
    }

    /// Append the message `message` to the message field of the VM status, and insert a seperator
    /// if the original message is non-empty.
    pub fn append_message_with_separator(
        mut self,
        separator: char,
        additional_message: String,
    ) -> Self {
        match self.0.message.as_mut() {
            Some(msg) => {
                if !msg.is_empty() {
                    msg.push(separator);
                }
                msg.push_str(&additional_message);
            }
            None => self.0.message = Some(additional_message),
        };
        self
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Location::Undefined => write!(f, "UNDEFINED"),
            Location::Script => write!(f, "Script"),
            Location::Module(id) => write!(f, "Module {:?}", id),
        }
    }
}

impl fmt::Display for PartialVMError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut status = format!("PartialVMError with status {:#?}", self.0.major_status);

        if let Some(sub_status) = self.0.sub_status {
            status = format!("{} with sub status {}", status, sub_status);
        }

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
/// Conversion functions from internal VM statuses into external VM statuses
////////////////////////////////////////////////////////////////////////////
impl From<VMError> for VMStatus {
    fn from(vm_error: VMError) -> VMStatus {
        vm_error.into_vm_status()
    }
}

pub fn vm_status_of_result<T>(result: VMResult<T>) -> VMStatus {
    match result {
        Ok(_) => VMStatus::Executed,
        Err(err) => err.into_vm_status(),
    }
}

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
        fmt::Debug::fmt(&self.0, f)
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
        } = self;
        f.debug_struct("PartialVMError")
            .field("major_status", major_status)
            .field("sub_status", sub_status)
            .field("message", message)
            .field("exec_state", exec_state)
            .field("indices", indices)
            .field("offsets", offsets)
            .finish()
    }
}

impl std::error::Error for PartialVMError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}
