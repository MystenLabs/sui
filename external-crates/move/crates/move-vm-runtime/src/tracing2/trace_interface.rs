use move_core_types::account_address::AccountAddress;
use move_vm_types::{loaded_data::runtime_types::Type, values::Value};

use crate::{
    interpreter::Frame,
    loader::{Function, Loader},
};

pub trait Tracer {
    fn name(&self) -> String;
    fn open_main_frame(
        &self,
        args: &[Value],
        ty_args: &[Type],
        function: &Function,
        loader: &Loader,
        remaining_gas: u64,
        link_context: AccountAddress,
    );
    fn close_main_frame(
        &self,
        ty_args: &[Type],
        return_values: &[Value],
        function: &Function,
        loader: &Loader,
        remaining_gas: u64,
        link_context: AccountAddress,
    );
    fn open_frame(
        &self,
        ty_args: &[Type],
        function: &Function,
        loader: &Loader,
        remaining_gas: u64,
        link_context: AccountAddress,
    );
    fn close_frame(
        &self,
        function: &Function,
        loader: &Loader,
        remaining_gas: u64,
        link_context: AccountAddress,
    );
    fn open_instruction(&self, frame: &Frame, loader: &Loader, remaining_gas: u64);
    fn close_instruction(&self, pc: u16, function: &Function, loader: &Loader, remaining_gas: u64);
}
