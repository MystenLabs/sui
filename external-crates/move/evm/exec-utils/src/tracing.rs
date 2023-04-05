// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use evm::Opcode;
use evm_runtime::tracing::{
    using as runtime_using, Event as RuntimeEvent, EventListener as RuntimeEventListener,
};

/// Enables tracing of the EVN runtime during the execution of `f`.
pub fn trace_runtime<R, F>(f: F) -> R
where
    F: FnOnce() -> R,
{
    let mut listener = RuntimeListener();
    runtime_using(&mut listener, f)
}

struct RuntimeListener();

impl RuntimeEventListener for RuntimeListener {
    fn event(&mut self, event: RuntimeEvent) {
        use RuntimeEvent::*;
        match event {
            Step { opcode, stack, .. } => {
                println!("{}", opc_name(opcode));
                println!("  stack:");
                for i in (0..stack.len()).rev() {
                    println!("    {}", stack.data()[i])
                }
            }
            StepResult {
                result,
                return_value,
                ..
            } => {
                println!("==> {:?} (ret={:?})", result, return_value)
            }
            SLoad { index, value, .. } => {
                println!("==> storage {} -> {}", index, value)
            }
            SStore { index, value, .. } => {
                println!("==> storage {} <- {}", index, value)
            }
        }
    }
}

/// Returns the name of an opcode.
///
/// Implementation remark: this should be in the evm-runtime crate, but did not find it.
/// We generated this by regular expression replacement of the definition in `evm_core::opcode`.
fn opc_name(code: Opcode) -> String {
    match code {
        // `STOP`
        Opcode::STOP => "STOP".to_string(),
        // `ADD`
        Opcode::ADD => "ADD".to_string(),
        // `MUL`
        Opcode::MUL => "MUL".to_string(),
        // `SUB`
        Opcode::SUB => "SUB".to_string(),
        // `DIV`
        Opcode::DIV => "DIV".to_string(),
        // `SDIV`
        Opcode::SDIV => "SDIV".to_string(),
        // `MOD`
        Opcode::MOD => "MOD".to_string(),
        // `SMOD`
        Opcode::SMOD => "SMOD".to_string(),
        // `ADDMOD`
        Opcode::ADDMOD => "ADDMOD".to_string(),
        // `MULMOD`
        Opcode::MULMOD => "MULMOD".to_string(),
        // `EXP`
        Opcode::EXP => "EXP".to_string(),
        // `SIGNEXTEND`
        Opcode::SIGNEXTEND => "SIGNEXTEND".to_string(),

        // `LT`
        Opcode::LT => "LT".to_string(),
        // `GT`
        Opcode::GT => "GT".to_string(),
        // `SLT`
        Opcode::SLT => "SLT".to_string(),
        // `SGT`
        Opcode::SGT => "SGT".to_string(),
        // `EQ`
        Opcode::EQ => "EQ".to_string(),
        // `ISZERO`
        Opcode::ISZERO => "ISZERO".to_string(),
        // `AND`
        Opcode::AND => "AND".to_string(),
        // `OR`
        Opcode::OR => "OR".to_string(),
        // `XOR`
        Opcode::XOR => "XOR".to_string(),
        // `NOT`
        Opcode::NOT => "NOT".to_string(),
        // `BYTE`
        Opcode::BYTE => "BYTE".to_string(),

        // `CALLDATALOAD`
        Opcode::CALLDATALOAD => "CALLDATALOAD".to_string(),
        // `CALLDATASIZE`
        Opcode::CALLDATASIZE => "CALLDATASIZE".to_string(),
        // `CALLDATACOPY`
        Opcode::CALLDATACOPY => "CALLDATACOPY".to_string(),
        // `CODESIZE`
        Opcode::CODESIZE => "CODESIZE".to_string(),
        // `CODECOPY`
        Opcode::CODECOPY => "CODECOPY".to_string(),

        // `SHL`
        Opcode::SHL => "SHL".to_string(),
        // `SHR`
        Opcode::SHR => "SHR".to_string(),
        // `SAR`
        Opcode::SAR => "SAR".to_string(),

        // `POP`
        Opcode::POP => "POP".to_string(),
        // `MLOAD`
        Opcode::MLOAD => "MLOAD".to_string(),
        // `MSTORE`
        Opcode::MSTORE => "MSTORE".to_string(),
        // `MSTORE8`
        Opcode::MSTORE8 => "MSTORE8".to_string(),
        // `JUMP`
        Opcode::JUMP => "JUMP".to_string(),
        // `JUMPI`
        Opcode::JUMPI => "JUMPI".to_string(),
        // `PC`
        Opcode::PC => "PC".to_string(),
        // `MSIZE`
        Opcode::MSIZE => "MSIZE".to_string(),
        // `JUMPDEST`
        Opcode::JUMPDEST => "JUMPDEST".to_string(),

        // `PUSHn`
        Opcode::PUSH1 => "PUSH1".to_string(),
        Opcode::PUSH2 => "PUSH2".to_string(),
        Opcode::PUSH3 => "PUSH3".to_string(),
        Opcode::PUSH4 => "PUSH4".to_string(),
        Opcode::PUSH5 => "PUSH5".to_string(),
        Opcode::PUSH6 => "PUSH6".to_string(),
        Opcode::PUSH7 => "PUSH7".to_string(),
        Opcode::PUSH8 => "PUSH8".to_string(),
        Opcode::PUSH9 => "PUSH9".to_string(),
        Opcode::PUSH10 => "PUSH10".to_string(),
        Opcode::PUSH11 => "PUSH11".to_string(),
        Opcode::PUSH12 => "PUSH12".to_string(),
        Opcode::PUSH13 => "PUSH13".to_string(),
        Opcode::PUSH14 => "PUSH14".to_string(),
        Opcode::PUSH15 => "PUSH15".to_string(),
        Opcode::PUSH16 => "PUSH16".to_string(),
        Opcode::PUSH17 => "PUSH17".to_string(),
        Opcode::PUSH18 => "PUSH18".to_string(),
        Opcode::PUSH19 => "PUSH19".to_string(),
        Opcode::PUSH20 => "PUSH20".to_string(),
        Opcode::PUSH21 => "PUSH21".to_string(),
        Opcode::PUSH22 => "PUSH22".to_string(),
        Opcode::PUSH23 => "PUSH23".to_string(),
        Opcode::PUSH24 => "PUSH24".to_string(),
        Opcode::PUSH25 => "PUSH25".to_string(),
        Opcode::PUSH26 => "PUSH26".to_string(),
        Opcode::PUSH27 => "PUSH27".to_string(),
        Opcode::PUSH28 => "PUSH28".to_string(),
        Opcode::PUSH29 => "PUSH29".to_string(),
        Opcode::PUSH30 => "PUSH30".to_string(),
        Opcode::PUSH31 => "PUSH31".to_string(),
        Opcode::PUSH32 => "PUSH32".to_string(),

        // `DUPn`
        Opcode::DUP1 => "DUP1".to_string(),
        Opcode::DUP2 => "DUP2".to_string(),
        Opcode::DUP3 => "DUP3".to_string(),
        Opcode::DUP4 => "DUP4".to_string(),
        Opcode::DUP5 => "DUP5".to_string(),
        Opcode::DUP6 => "DUP6".to_string(),
        Opcode::DUP7 => "DUP7".to_string(),
        Opcode::DUP8 => "DUP8".to_string(),
        Opcode::DUP9 => "DUP9".to_string(),
        Opcode::DUP10 => "DUP10".to_string(),
        Opcode::DUP11 => "DUP11".to_string(),
        Opcode::DUP12 => "DUP12".to_string(),
        Opcode::DUP13 => "DUP13".to_string(),
        Opcode::DUP14 => "DUP14".to_string(),
        Opcode::DUP15 => "DUP15".to_string(),
        Opcode::DUP16 => "DUP16".to_string(),

        // `SWAPn`
        Opcode::SWAP1 => "SWAP1".to_string(),
        Opcode::SWAP2 => "SWAP2".to_string(),
        Opcode::SWAP3 => "SWAP3".to_string(),
        Opcode::SWAP4 => "SWAP4".to_string(),
        Opcode::SWAP5 => "SWAP5".to_string(),
        Opcode::SWAP6 => "SWAP6".to_string(),
        Opcode::SWAP7 => "SWAP7".to_string(),
        Opcode::SWAP8 => "SWAP8".to_string(),
        Opcode::SWAP9 => "SWAP9".to_string(),
        Opcode::SWAP10 => "SWAP10".to_string(),
        Opcode::SWAP11 => "SWAP11".to_string(),
        Opcode::SWAP12 => "SWAP12".to_string(),
        Opcode::SWAP13 => "SWAP13".to_string(),
        Opcode::SWAP14 => "SWAP14".to_string(),
        Opcode::SWAP15 => "SWAP15".to_string(),
        Opcode::SWAP16 => "SWAP16".to_string(),

        // `RETURN`
        Opcode::RETURN => "RETURN".to_string(),
        // `REVERT`
        Opcode::REVERT => "REVERT".to_string(),

        // `INVALID`
        Opcode::INVALID => "INVALID".to_string(),

        // `SHA3`
        Opcode::SHA3 => "SHA3".to_string(),
        // `ADDRESS`
        Opcode::ADDRESS => "ADDRESS".to_string(),
        // `BALANCE`
        Opcode::BALANCE => "BALANCE".to_string(),
        // `SELFBALANCE`
        Opcode::SELFBALANCE => "SELFBALANCE".to_string(),
        // `BASEFEE`
        Opcode::BASEFEE => "BASEFEE".to_string(),
        // `ORIGIN`
        Opcode::ORIGIN => "ORIGIN".to_string(),
        // `CALLER`
        Opcode::CALLER => "CALLER".to_string(),
        // `CALLVALUE`
        Opcode::CALLVALUE => "CALLVALUE".to_string(),
        // `GASPRICE`
        Opcode::GASPRICE => "GASPRICE".to_string(),
        // `EXTCODESIZE`
        Opcode::EXTCODESIZE => "EXTCODESIZE".to_string(),
        // `EXTCODECOPY`
        Opcode::EXTCODECOPY => "EXTCODECOPY".to_string(),
        // `EXTCODEHASH`
        Opcode::EXTCODEHASH => "EXTCODEHASH".to_string(),
        // `RETURNDATASIZE`
        Opcode::RETURNDATASIZE => "RETURNDATASIZE".to_string(),
        // `RETURNDATACOPY`
        Opcode::RETURNDATACOPY => "RETURNDATACOPY".to_string(),
        // `BLOCKHASH`
        Opcode::BLOCKHASH => "BLOCKHASH".to_string(),
        // `COINBASE`
        Opcode::COINBASE => "COINBASE".to_string(),
        // `TIMESTAMP`
        Opcode::TIMESTAMP => "TIMESTAMP".to_string(),
        // `NUMBER`
        Opcode::NUMBER => "NUMBER".to_string(),
        // `DIFFICULTY`
        Opcode::DIFFICULTY => "DIFFICULTY".to_string(),
        // `GASLIMIT`
        Opcode::GASLIMIT => "GASLIMIT".to_string(),
        // `SLOAD`
        Opcode::SLOAD => "SLOAD".to_string(),
        // `SSTORE`
        Opcode::SSTORE => "SSTORE".to_string(),
        // `GAS`
        Opcode::GAS => "GAS".to_string(),
        // `LOGn`
        Opcode::LOG0 => "LOG0".to_string(),
        Opcode::LOG1 => "LOG1".to_string(),
        Opcode::LOG2 => "LOG2".to_string(),
        Opcode::LOG3 => "LOG3".to_string(),
        Opcode::LOG4 => "LOG4".to_string(),
        // `CREATE`
        Opcode::CREATE => "CREATE".to_string(),
        // `CREATE2`
        Opcode::CREATE2 => "CREATE2".to_string(),
        // `CALL`
        Opcode::CALL => "CALL".to_string(),
        // `CALLCODE`
        Opcode::CALLCODE => "CALLCODE".to_string(),
        // `DELEGATECALL`
        Opcode::DELEGATECALL => "DELEGATECALL".to_string(),
        // `STATICCALL`
        Opcode::STATICCALL => "STATICCALL".to_string(),
        // `SUICIDE`
        Opcode::SUICIDE => "SUICIDE".to_string(),
        // `CHAINID`
        Opcode::CHAINID => "CHAINID".to_string(),
        _ => format!("opc #{:x}", code.as_usize()),
    }
}
