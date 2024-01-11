use crate::compiler::{as_module, compile_units};
use expect_test::expect;
use move_binary_format::errors::VMResult;
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::ModuleId,
    trace::CallTrace,
    value::{MoveStruct, MoveValue},
};
use move_vm_runtime::move_vm::MoveVM;
use move_vm_test_utils::InMemoryStorage;
use move_vm_types::gas::UnmeteredGasMeter;
use move_vm_types::loaded_data::runtime_types::Type;

const TEST_ADDR: AccountAddress = AccountAddress::new([42; AccountAddress::LENGTH]);
const TEST_MODULE_ID: &str = "M";

#[test]
fn call_traces_collection() {
    let code = code();
    let traces = format_traces(
        run(
            &code,
            "test",
            vec![MoveValue::U64(42), MoveValue::Bool(true)],
            vec![Type::U256],
        )
        .unwrap(),
    );

    expect![[r#"
        1: fun test(U64(42), Bool(true))
        2:  fun test2(U64(42), Bool(true))
        3:   fun test3_mut(Foo [(Identifier("x"), U64(42)), (Identifier("y"), Bool(true))])
        4:    fun test4_mut(U64(42))
        3:   fun test3(Foo [(Identifier("x"), U64(43)), (Identifier("y"), Bool(true))])
        3:   fun test3_mut(Foo [(Identifier("x"), U64(43)), (Identifier("y"), Bool(true))])
        4:    fun test4_mut(U64(43))
        3:   fun test3(Foo [(Identifier("x"), U64(44)), (Identifier("y"), Bool(true))])
        3:   fun test5(Foo [(Identifier("x"), U64(44)), (Identifier("y"), Bool(true))])"#]]
    .assert_eq(&traces);
}

fn code() -> ModuleCode {
    let code = format!(
        r#"
        module 0x{}::{} {{
            struct Foo has drop {{ x: u64, y: bool }}

            public fun test<T>(x: u64, y: bool) {{
                test2(x, y);
            }}

            fun test2(x: u64, y: bool) {{
                let f = Foo {{ x, y }};

                let i = 0;

                while (i < 2) {{
                    test3_mut(&mut f);
                    test3(&f);
                    i = i + 1;
                }};

                test5(f);
            }}

            // Foo is `ContainerRef`
            fun test3(f: &Foo) {{
                let _ = f.x;
            }}

            // Foo is `ContainerRef`
            fun test3_mut(f: &mut Foo) {{
                test4_mut(&mut f.x);
            }}

            // x is `IndexedRef`
            fun test4_mut(x: &mut u64) {{
                *x = *x + 1;
            }}

            // x is `Container`
            fun test5(_f: Foo) {{ }}
        }}
    "#,
        TEST_ADDR, TEST_MODULE_ID,
    );

    let module_id = ModuleId::new(TEST_ADDR, Identifier::new(TEST_MODULE_ID).unwrap());

    (module_id, code)
}

fn run(
    module: &ModuleCode,
    fun_name: &str,
    args: Vec<MoveValue>,
    ty_args: Vec<Type>,
) -> VMResult<Vec<CallTrace>> {
    let module_id = &module.0;
    let modules = vec![module.clone()];
    let (vm, storage) = setup_vm(&modules);
    let mut session = vm.new_session(&storage);

    let fun_name = Identifier::new(fun_name).unwrap();

    let args: Vec<_> = args
        .into_iter()
        .map(|val| val.simple_serialize().unwrap())
        .collect();

    session
        .execute_function_bypass_visibility(
            module_id,
            &fun_name,
            ty_args,
            args,
            &mut UnmeteredGasMeter,
        )
        .map(|ret_values| ret_values.call_traces)
}

type ModuleCode = (ModuleId, String);

fn setup_vm(modules: &[ModuleCode]) -> (MoveVM, InMemoryStorage) {
    let mut storage = InMemoryStorage::new();
    compile_modules(&mut storage, modules);
    (MoveVM::new(vec![]).unwrap(), storage)
}

fn compile_modules(storage: &mut InMemoryStorage, modules: &[ModuleCode]) {
    modules.iter().for_each(|(id, code)| {
        compile_module(storage, id, code);
    });
}

fn compile_module(storage: &mut InMemoryStorage, mod_id: &ModuleId, code: &str) {
    let mut units = compile_units(code).unwrap();
    let module = as_module(units.pop().unwrap());
    let mut blob = vec![];
    module.serialize(&mut blob).unwrap();
    storage.publish_or_overwrite_module(mod_id.clone(), blob);
}

fn format_traces(call_traces: Vec<CallTrace>) -> String {
    let formatted_traces: Vec<_> = call_traces
        .into_iter()
        .map(|trace| {
            let ident = trace.function.as_str();
            let args: Vec<_> = trace
                .args
                .into_iter()
                .map(|arg| match &*arg {
                    MoveValue::Struct(MoveStruct::WithTypes { type_, fields }) => {
                        let ident = type_.name.as_str();

                        format!("{} {:?}", ident, fields)
                    }
                    arg => format!("{:?}", arg),
                })
                .collect();

            format!(
                "{}:{}fun {ident}({})",
                trace.depth,
                " ".repeat(trace.depth as usize),
                args.join(", ")
            )
        })
        .collect();

    formatted_traces.join("\n")
}
