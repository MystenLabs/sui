use afl::fuzz;

use transaction_fuzzer::account_universe::AccountCurrent;
use transaction_fuzzer::account_universe::AccountData;
use transaction_fuzzer::type_arg_fuzzer::run_type_tags;
use transaction_fuzzer::{executor::Executor, type_arg_fuzzer::gen_type_tag};
use move_core_types::language_storage::TypeTag;

pub fn main() {
    fuzz!(|type_tag: TypeTag| {
        let mut exec = Executor::new();
        let account = AccountCurrent::new(AccountData::new_random());
        run_type_tags(&account, &mut exec, vec![type_tag]);
    });
}
