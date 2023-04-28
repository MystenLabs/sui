use afl::fuzz;
use move_core_types::value::MoveValue;
use sui_json_rpc_types::SuiMoveValue;

pub fn main() {
    fuzz!(|move_value: MoveValue| {
        SuiMoveValue::from(move_value);
    });
}
