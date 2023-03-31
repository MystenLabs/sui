This crate contains an extension to the Move language with large-scale storage tables.

In order to use this extension with the Move CLI and package system, you need to compile with
`feature = ["table-extension"]`.

In order to use this extension in your adapter, you do something as follows:

```rust
use move_core_types::account_address::AccountAddress;
use move_stdlib::natives;
use move_table_extension::NativeTableContext;
use move_vm_runtime::move_vm::MoveVM;
use move_vm_runtime::native_functions::NativeContextExtensions;

fn run() {
    let resource_resolver = unimplemented!(); // a resource resolver the adapter provides
    let txn_hash = unimplemented!(); // a unique hash for table creation for this transaction
    let table_resolver = unimplemented!(); // a remote table resolver the adapter provides
    let std_addr = unimplemented!(); // address where to deploy the std lib
    let extension_addr = unimplemented!(); // address where to deploy the table extension

    let mut extensions = NativeContextExtensions::default();
    extensions.add(NativeTableContext::new(txn_hash, table_resolver));
    let mut natives = move_stdlib::natives::all_natives(std_addr);
    natives.append(&mut move_table_extension::table_natives(extension_addr));
    let vm = MoveVM::new(natives);

    let session = vm.new_session_with_extensions(resource_resolver, extensions);
    let result = session.execute_function(..)?;
    let (change_set, events, extensions) = session.finish_with_extensions()?;
    let table_change_set = extensions.get::<NativeTableContext>().into_change_set();

    // Do something with the table change set
    // ...
}
```
