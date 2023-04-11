// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use codespan_reporting::{diagnostic::Severity, term::termcolor::Buffer};
use itertools::Itertools;
use move_core_types::{
    identifier::IdentStr, language_storage::ModuleId, metadata::Metadata, value::MoveValue,
};
use std::collections::{BTreeMap, BTreeSet};

use move_model::{
    emit, emitln,
    model::{FunId, FunctionEnv, GlobalEnv, Loc, QualifiedId, QualifiedInstId},
    ty::Type,
};

use crate::{
    abi_move_metadata::generate_abi_move_metadata,
    abi_signature::{from_event_sig, from_solidity_sig},
    attributes,
    context::Context,
    functions::FunctionGenerator,
    solidity_ty::SoliditySignature,
    yul_functions::YulFunction,
    Options,
};

use crate::context::Contract;
use move_model::model::{ModuleEnv, StructId};
use sha3::{Digest, Keccak256};

/// Mutable state of the generator.
#[derive(Default)]
pub struct Generator {
    // Location of the currently compiled contract, for general error messages.
    pub(crate) contract_loc: Loc,
    // If the currently compiled contract has a storage type, its contained here.
    pub(crate) storage_type: Option<QualifiedInstId<StructId>>,
    /// Move functions, including type instantiation, needed in the currently generated code block.
    needed_move_functions: Vec<QualifiedInstId<FunId>>,
    /// Move functions for which code has been emitted.
    done_move_functions: BTreeSet<QualifiedInstId<FunId>>,
    /// Yule functions needed in the currently generated code block.
    needed_yul_functions: BTreeSet<YulFunction>,
    /// Auxiliary functions needed in the current block.
    needed_auxiliary_functions: Vec<(String, Box<AuxilaryFunctionGenerator>)>,
    /// Auxiliary functions for which code has been emitted.
    done_auxiliary_functions: BTreeSet<String>,
    /// Mapping of type signature hash to type, to identify collisions.
    pub(crate) type_sig_map: BTreeMap<u32, Type>,
    /// Solidity signature for callable functions for generating JSON-ABI
    pub(crate) solidity_sigs: Vec<(SoliditySignature, attributes::FunctionAttribute)>,
    /// Solidity signature for the optional constructor for generating JSON-ABI
    pub(crate) constructor_sig: Option<SoliditySignature>,
}

type AuxilaryFunctionGenerator = dyn FnOnce(&mut Generator, &Context);

// ================================================================================================
// Entry point

impl Generator {
    /// Run the generator and produce a lisy of triples of contract name, Yul contract object, and ABI.
    pub fn run(options: &Options, env: &GlobalEnv) -> Vec<(String, String, String)> {
        let mut res = vec![];
        let ctx = &Context::new(options, env, false);
        for contract in ctx.derive_contracts() {
            let module = &ctx.env.get_module(contract.module);
            if !module.is_target() {
                // Ignore contract from module not target of compilation
                continue;
            }
            let mut gen = Generator::default();
            gen.contract_object(ctx, &contract);
            res.push((
                contract.name,
                ctx.writer.extract_result(),
                ctx.abi_writer.extract_result(),
            ))
        }
        res
    }

    // Run the generator for evm unit tests and produce a mapping from function id to Yul test object.
    pub fn run_for_evm_tests(
        options: &Options,
        env: &GlobalEnv,
    ) -> BTreeMap<QualifiedId<FunId>, String> {
        let mut res = BTreeMap::new();
        let ctx = Context::new(options, env, /*for_test*/ true);

        // Go over all evm_test functions which are in modules which are target of compilation,
        // and generate a test object for them.
        for module in env.get_modules() {
            if !module.is_target() {
                continue;
            }
            for fun in module.get_functions() {
                if attributes::is_evm_test_fun(&fun) {
                    let mut gen = Generator::default();
                    gen.test_object(&ctx, &fun, &[]);
                    res.insert(fun.get_qualified_id(), ctx.writer.extract_result());
                }
            }
        }

        res
    }

    /// Run the generator for a specific unit test and generate a Yul test object for it.
    /// Return diagnostics if errors are raised.
    pub fn run_for_unit_test(
        options: &Options,
        env: &GlobalEnv,
        module_id: &ModuleId,
        fun_name: &IdentStr,
        args: &[MoveValue],
    ) -> Result<String, String> {
        let fun = env
            .find_function_by_language_storage_id_name(module_id, fun_name)
            .unwrap_or_else(|| {
                panic!(
                    "Failed to find test function {}::{}. This should not have happened.",
                    module_id, fun_name
                )
            });

        let ctx = Context::new(options, env, /*for_test*/ true);
        let mut gen = Generator::default();
        gen.test_object(&ctx, &fun, args);
        if ctx.env.has_errors() {
            let mut buffer = Buffer::no_color();
            ctx.env.report_diag(&mut buffer, Severity::Error);
            Err(String::from_utf8_lossy(buffer.as_slice()).to_string())
        } else {
            Ok(ctx.writer.extract_result())
        }
    }

    /// Generate metadata
    pub(crate) fn generate_abi_metadata(options: &Options, env: &GlobalEnv) -> Vec<Metadata> {
        let ctx = &Context::new(options, env, false);
        let mut meta_vec = vec![];
        for contract in ctx.derive_contracts() {
            let module = &ctx.env.get_module(contract.module);
            if !module.is_target() {
                // Ignore contract from module not target of compilation
                continue;
            }
            let mut gen = Generator::default();
            gen.compute_ethereum_signatures(ctx, &contract);
            let metadata = generate_abi_move_metadata(
                ctx,
                contract.receive.is_some(),
                contract.fallback.is_some(),
            );
            meta_vec.push(metadata);
        }
        meta_vec
    }
}

// ================================================================================================
// Object generation

impl Generator {
    /// Generate contract object for given contract functions.
    fn contract_object(&mut self, ctx: &Context, contract: &Contract) {
        self.header(ctx);
        // Initialize contract specific state
        let module = &ctx.env.get_module(contract.module);
        self.contract_loc = module.get_loc();
        self.storage_type = contract
            .storage
            .map(|struct_id| contract.module.qualified_inst(struct_id, vec![]));
        // Start generating Yul object.
        emit!(ctx.writer, "object \"{}\" ", contract.name);
        ctx.emit_block(|| {
            // Generate the deployment code block
            self.begin_code_block(ctx);
            self.optional_create(ctx, module, contract);
            let contract_deployed_name = format!("{}_deployed", contract.name);
            emitln!(
                ctx.writer,
                "codecopy(0, dataoffset(\"{}\"), datasize(\"{}\"))",
                contract_deployed_name,
                contract_deployed_name
            );
            emitln!(
                ctx.writer,
                "return(0, datasize(\"{}\"))",
                contract_deployed_name,
            );
            self.end_code_block(ctx);

            // Generate the runtime object
            emit!(ctx.writer, "object \"{}\" ", contract_deployed_name);
            ctx.emit_block(|| {
                self.begin_code_block(ctx);
                emitln!(
                    ctx.writer,
                    "mstore(${MEM_SIZE_LOC}, memoryguard(${USED_MEM}))"
                );
                let callables = contract
                    .callables
                    .iter()
                    .map(|f| module.get_function(*f))
                    .collect_vec();
                let receiver = contract.receive.map(|f| module.get_function(f));
                let fallback = contract.fallback.map(|f| module.get_function(f));
                self.callable_functions(ctx, &callables, receiver, fallback);
                self.end_code_block(ctx);
            })
        });
        // Generate JSON-ABI
        self.generate_abi_string(ctx);
    }

    /// Compute ethereum signatures and returns whether
    pub(crate) fn compute_ethereum_signatures(&mut self, ctx: &Context, contract: &Contract) {
        let module = &ctx.env.get_module(contract.module);
        let constructor_opt = contract.constructor.map(|f| module.get_function(f));
        if let Some(constructor) = constructor_opt {
            let solidity_sig_constructor = self.get_solidity_signature(ctx, &constructor, false);
            if let Some(fun_attr_opt) = attributes::construct_fun_attribute(&constructor) {
                ctx.build_constructor(&solidity_sig_constructor, fun_attr_opt, &constructor);
            }
        }

        let callables = contract
            .callables
            .iter()
            .map(|f| module.get_function(*f))
            .collect_vec();

        for fun in callables {
            if !self.is_suitable_for_dispatch(ctx, &fun) {
                ctx.env.diag(
                    Severity::Warning,
                    &fun.get_loc(),
                    "cannot dispatch this function because of unsupported parameter types",
                );
                continue;
            }
            let sig = self.get_solidity_signature(ctx, &fun, true);
            if let Some(fun_attr_opt) = attributes::construct_fun_attribute(&fun) {
                ctx.build_callable_signature_map(&sig, fun_attr_opt, &fun);
            } else {
                ctx.env.error(
                    &fun.get_loc(),
                    "callable functions can only have one attribute among payable, pure and view",
                );
            }
        }
    }

    /// Generate JSON-ABI
    fn generate_abi_string(&self, ctx: &Context) {
        let mut res = vec![];
        let event_sigs = ctx
            .event_signature_map
            .borrow()
            .values()
            .cloned()
            .collect_vec();
        for sig in &event_sigs {
            res.push(serde_json::to_string_pretty(&from_event_sig(sig)).unwrap());
        }
        for (sig, attr) in &self.solidity_sigs {
            res.push(
                serde_json::to_string_pretty(&from_solidity_sig(sig, Some(*attr), "function"))
                    .unwrap(),
            );
        }
        if let Some(constructor) = &self.constructor_sig {
            res.push(
                serde_json::to_string_pretty(&from_solidity_sig(constructor, None, "constructor"))
                    .unwrap(),
            );
        }
        emitln!(ctx.abi_writer, "[");
        emitln!(
            ctx.abi_writer,
            "{}",
            res.iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>()
                .join(",\n")
        );
        emitln!(ctx.abi_writer, "]");
    }

    /// Generate test object for given function.
    ///
    /// A test object contains no nested objects and is intended to execute at transaction time,
    /// without actually deploying any contract code.
    fn test_object(&mut self, ctx: &Context, test: &FunctionEnv, args: &[MoveValue]) {
        self.header(ctx);
        ctx.check_no_generics(test);
        if test.get_return_count() > 0 {
            ctx.env
                .error(&test.get_loc(), "test functions cannot have return values");
            return;
        }
        if test.get_parameter_count() != args.len() {
            ctx.env.error(
                &test.get_loc(),
                &format!(
                    "test function has {} parameters but {} were provided",
                    test.get_parameter_count(),
                    args.len()
                ),
            );
            return;
        }
        for ty in test.get_parameter_types() {
            if !ty.is_signer_or_address() {
                ctx.env.error(
                    &test.get_loc(),
                    "only signer or address parameters are allowed currently",
                );
                return;
            }
        }

        let fun_id = test.get_qualified_id().instantiate(vec![]);
        let test_contract_name = format!("test_{}", ctx.make_function_name(&fun_id));
        emit!(ctx.writer, "object \"{}\" ", test_contract_name);
        ctx.emit_block(|| {
            self.begin_code_block(ctx);
            emitln!(
                ctx.writer,
                "mstore(${MEM_SIZE_LOC}, memoryguard(${USED_MEM}))"
            );
            self.need_move_function(&fun_id);

            for (idx, arg) in args.iter().enumerate() {
                emit!(ctx.writer, "let $arg{} := ", idx);
                match arg {
                    MoveValue::Address(addr) => {
                        emitln!(ctx.writer, "{}", addr.to_hex_literal());
                    }
                    _ => unreachable!(
                        "only address literals are allowed as test arguments currently"
                    ),
                }
            }

            let fun_name = ctx.make_function_name(&fun_id);
            emit!(ctx.writer, "{}(", fun_name);
            for idx in 0..args.len() {
                if idx > 0 {
                    emit!(ctx.writer, ", ");
                }
                emit!(ctx.writer, "$arg{}", idx);
            }
            emitln!(ctx.writer, ")");

            emitln!(ctx.writer, "return (0, 0)");
            self.end_code_block(ctx);
        });
    }

    /// Generate header for output Yul.
    fn header(&mut self, ctx: &Context) {
        emitln!(
            ctx.writer,
            "\
/* =======================================
 * Generated by Move-To-Yul compiler v{}
 * ======================================= */",
            ctx.options.version(),
        );
        emitln!(ctx.writer);
        if ctx.options.generate_source_info() {
            let mut use_src_emitted = false;
            for (file_no, file_path) in ctx
                .file_id_map
                .values()
                .sorted_by(|(n1, _), (n2, _)| n1.cmp(n2))
            {
                let use_str = format!("{}:\"{}\"", file_no, file_path);
                if !use_src_emitted {
                    emitln!(ctx.writer, "/// @use-src {}", use_str);
                    use_src_emitted = true;
                } else {
                    emitln!(ctx.writer, "///        , {}", use_str)
                }
            }
            emitln!(ctx.writer);
        }
        emitln!(ctx.writer);
    }

    /// Generate optional creator (contract constructor).
    fn optional_create(&mut self, ctx: &Context, module: &ModuleEnv, contract: &Contract) {
        if let Some(creator_id) = contract.constructor {
            let creator = module.get_function(creator_id);
            ctx.check_no_generics(&creator);
            if let Some(storage) = &self.storage_type {
                // The creator function must return a value of the storage type.
                let storage_ty = storage.to_type();
                if creator.get_return_count() != 1 || creator.get_return_type(0) != storage_ty {
                    ctx.env.error(
                        &creator.get_loc(),
                        &format!("creator function for contract with #[storage] must return value of type `{}`", storage_ty.display(&ctx.env.get_type_display_ctx()))
                    )
                }
            } else if creator.get_return_count() > 0 {
                ctx.env.error(
                    &creator.get_loc(),
                    "return values not allowed for creator functions without specified #[storage]",
                )
            }

            if !self.is_suitable_for_dispatch(ctx, &creator) {
                ctx.env.error(
                    &creator.get_loc(),
                    "creator function has unsupported parameter types",
                );
            }

            emitln!(
                ctx.writer,
                "mstore(${MEM_SIZE_LOC}, memoryguard(${USED_MEM}))"
            );

            // Translate call to the constructor function
            let fun_id = creator.get_qualified_id().instantiate(vec![]);
            let function_name = ctx.make_function_name(&fun_id);
            let solidity_sig = self.get_solidity_signature(ctx, &creator, false);
            self.constructor_sig = Some(solidity_sig.clone());
            let param_count = solidity_sig.para_types.len();
            let mut params = "".to_string();
            if param_count > 0 {
                let program_size_str = "program_size".to_string();
                let arg_size_str = "arg_size".to_string();
                let memory_data_offset_str = "memory_data_offset".to_string();
                emitln!(
                    ctx.writer,
                    "let {} := datasize(\"{}\")",
                    program_size_str,
                    contract.name
                );
                emitln!(
                    ctx.writer,
                    "let {} := sub(codesize(), {})",
                    arg_size_str,
                    program_size_str
                );
                let malloc_call = self.call_builtin_str(
                    ctx,
                    YulFunction::Malloc,
                    std::iter::once(arg_size_str.clone()),
                );
                emitln!(
                    ctx.writer,
                    "let {} := {}",
                    memory_data_offset_str,
                    malloc_call
                );
                emitln!(
                    ctx.writer,
                    "codecopy({}, {}, {})",
                    memory_data_offset_str,
                    program_size_str,
                    arg_size_str
                );
                let decoding_fun_name = self.generate_abi_tuple_decoding_para(
                    ctx,
                    &solidity_sig,
                    creator.get_parameter_types(),
                    true,
                );
                params = (0..param_count).map(|i| format!("param_{}", i)).join(", ");
                let let_params = format!("let {} := ", params);
                emitln!(
                    ctx.writer,
                    "{}{}({}, add({}, {}))",
                    let_params,
                    decoding_fun_name,
                    memory_data_offset_str,
                    memory_data_offset_str,
                    arg_size_str
                );
            }

            // Call the function
            if let Some(storage_id) = contract.storage {
                // The creator function returns a value which we need to store as
                // a resource.
                emitln!(
                    ctx.writer,
                    "let $new_value := {}({})",
                    function_name,
                    params
                );
                let storage = module.get_id().qualified_inst(storage_id, vec![]);
                self.move_to_addr(
                    ctx,
                    storage,
                    "address()".to_string(),
                    "$new_value".to_string(),
                );
            } else {
                // Otherwise the creator function is responsible to store initialized data itself.
                emitln!(ctx.writer, "{}({})", function_name, params);
            }

            self.need_move_function(&fun_id);
        }
    }

    /// Generate Yul definitions for all callable functions.
    fn callable_functions(
        &mut self,
        ctx: &Context,
        callables: &[FunctionEnv<'_>],
        receiver: Option<FunctionEnv<'_>>,
        fallback: Option<FunctionEnv<'_>>,
    ) {
        self.generate_dispatcher_routine(ctx, callables, &receiver, &fallback);
        for fun in callables {
            ctx.check_no_generics(fun);
            self.function(ctx, &fun.get_qualified_id().instantiate(vec![]))
        }
        if let Some(fun) = &receiver {
            ctx.check_no_generics(fun);
            self.function(ctx, &fun.get_qualified_id().instantiate(vec![]))
        }
        if let Some(fun) = &fallback {
            ctx.check_no_generics(fun);
            self.function(ctx, &fun.get_qualified_id().instantiate(vec![]))
        }
    }

    /// Generate code for a function. This delegates to the function generator.
    fn function(&mut self, ctx: &Context, fun_id: &QualifiedInstId<FunId>) {
        self.done_move_functions.insert(fun_id.clone());
        FunctionGenerator::run(self, ctx, fun_id)
    }

    /// Begin a new code block.
    fn begin_code_block(&mut self, ctx: &Context) {
        assert!(self.needed_move_functions.is_empty());
        assert!(self.needed_yul_functions.is_empty());
        emitln!(ctx.writer, "code {");
        ctx.writer.indent();
    }

    /// End a code block, generating all functions needed by top-level callable functions.
    fn end_code_block(&mut self, ctx: &Context) {
        // Before the end of the code block, we need to emit definitions of all
        // functions reached by callable entry points. While we traversing this list,
        // more functions might be added due to transitive calls.
        while let Some(fun_id) = self.needed_move_functions.pop() {
            if !self.done_move_functions.contains(&fun_id) {
                self.function(ctx, &fun_id)
            }
        }

        // We also need to emit code for all needed auxiliary functions.
        while let Some((function_name, generator)) = self.needed_auxiliary_functions.pop() {
            if !self.done_auxiliary_functions.contains(&function_name) {
                emit!(ctx.writer, "function {}", function_name);
                self.done_auxiliary_functions.insert(function_name);
                generator(self, ctx)
            }
        }

        // We finally emit code for all Yul functions which have been needed by the Move
        // or auxiliary functions.
        for fun in &self.needed_yul_functions {
            emitln!(ctx.writer, &fun.yule_def());
        }
        // Empty the set of functions for next block.
        self.done_move_functions.clear();
        self.needed_yul_functions.clear();
        self.done_auxiliary_functions.clear();
        ctx.writer.unindent();
        emitln!(ctx.writer, "}")
    }
}

// ================================================================================================
// Helpers shared with other modules

impl Generator {
    /// Generate call to a builtin function.
    pub(crate) fn call_builtin(
        &mut self,
        ctx: &Context,
        fun: YulFunction,
        args: impl Iterator<Item = String>,
    ) {
        emitln!(ctx.writer, "{}", self.call_builtin_str(ctx, fun, args))
    }

    /// Generate call to a builtin function which delivers results.
    pub(crate) fn call_builtin_with_result(
        &mut self,
        ctx: &Context,
        prefix: &str,
        mut results: impl Iterator<Item = String>,
        fun: YulFunction,
        args: impl Iterator<Item = String>,
    ) {
        emitln!(
            ctx.writer,
            "{}{} := {}",
            prefix,
            results.join(", "),
            self.call_builtin_str(ctx, fun, args)
        )
    }

    /// Create the string representing call to builtin function.
    pub(crate) fn call_builtin_str(
        &mut self,
        _ctx: &Context,
        fun: YulFunction,
        mut args: impl Iterator<Item = String>,
    ) -> String {
        self.need_yul_function(fun);
        for dep in fun.yule_deps() {
            self.needed_yul_functions.insert(dep);
        }
        format!("{}({})", fun.yule_name(), args.join(", "))
    }

    /// Indicate that a Yul function is needed.
    pub(crate) fn need_yul_function(&mut self, yul_fun: YulFunction) {
        if !self.needed_yul_functions.contains(&yul_fun) {
            self.needed_yul_functions.insert(yul_fun);
            for dep in yul_fun.yule_deps() {
                self.need_yul_function(dep);
            }
        }
    }

    /// Indicate that an auxiliary function of name is needed. Return the name.
    pub(crate) fn need_auxiliary_function(
        &mut self,
        function_name: String,
        generator: Box<AuxilaryFunctionGenerator>,
    ) -> String {
        if !self.done_auxiliary_functions.contains(&function_name) {
            self.needed_auxiliary_functions
                .push((function_name.clone(), generator));
        }
        function_name
    }

    /// Indicate that a move function is needed.
    pub(crate) fn need_move_function(&mut self, fun_id: &QualifiedInstId<FunId>) {
        if !self.done_move_functions.contains(fun_id) {
            self.needed_move_functions.push(fun_id.clone())
        }
    }

    pub(crate) fn equality_function(&mut self, ctx: &Context, ty: Type) -> String {
        let function_name = format!("$Eq_{}", ctx.mangle_types(&[ty.clone()]));
        if ctx.type_allocates_memory(&ty) {
            let generate_fun = move |gen: &mut Generator, ctx: &Context| {
                emitln!(ctx.writer, "(x, y) -> res");
                ctx.emit_block(|| {
                    if ty.is_vector() {
                        crate::vectors::equality_fun(gen, ctx, &ty)
                    } else if ctx.type_is_struct(&ty) {
                        struct_equality_fun(gen, ctx, &ty)
                    }
                });
            };
            self.need_auxiliary_function(function_name, Box::new(generate_fun))
        } else {
            self.need_yul_function(YulFunction::Eq);
            YulFunction::Eq.yule_name()
        }
    }

    /// Copy literal string to memory
    pub(crate) fn copy_literal_to_memory(&mut self, value: Vec<u8>) -> String {
        let name_prefix = "copy_literal_string_to_memory";
        let function_name = format!("{}_{}", name_prefix, self.vector_u8_hash(&value));
        let value = value.clone();
        let generate_fun = move |gen: &mut Generator, ctx: &Context| {
            emit!(ctx.writer, "(value) ");
            ctx.emit_block(|| {
                for c in value {
                    let store_u8_str = gen.call_builtin_str(
                        ctx,
                        YulFunction::MemoryStoreU8,
                        vec!["value".to_string(), c.to_string()].into_iter(),
                    );
                    emitln!(ctx.writer, "{}", store_u8_str);
                    emitln!(ctx.writer, "value := add(value, 1)");
                }
            });
        };
        self.need_auxiliary_function(function_name, Box::new(generate_fun))
    }

    fn vector_u8_hash(&mut self, vec: &[u8]) -> u32 {
        let mut keccak = Keccak256::new();
        keccak.update(vec);
        let digest = keccak.finalize();
        u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]])
    }

    pub(crate) fn type_storage_base(
        &mut self,
        ctx: &Context,
        ty: &Type,
        category: &str,
        instance: String,
    ) -> String {
        let hash = self.type_hash(ctx, ty);
        self.call_builtin_str(
            ctx,
            YulFunction::MakeTypeStorageBase,
            vec![category.to_string(), format!("0x{:x}", hash), instance].into_iter(),
        )
    }

    /// Derive a 4 byte hash for a type. If this hash creates a collision in the current
    /// contract, create an error.
    pub(crate) fn type_hash(&mut self, ctx: &Context, ty: &Type) -> u32 {
        let sig = ctx.mangle_type(ty);
        let mut keccak = Keccak256::new();
        keccak.update(sig.as_bytes());
        let digest = keccak.finalize();
        let hash = u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]]);
        if let Some(old_ty) = self.type_sig_map.insert(hash, ty.clone()) {
            if old_ty != *ty {
                let ty_ctx = &ctx.env.get_type_display_ctx();
                ctx.env.error(
                    &self.contract_loc,
                    &format!(
                        "collision of type hash for types `{}` and `{}`\n\
                         (resolution via attribute not yet implemented)",
                        ty.display(ty_ctx),
                        old_ty.display(ty_ctx)
                    ),
                )
            }
        }
        hash
    }
}

fn struct_equality_fun(gen: &mut Generator, ctx: &Context, ty: &Type) {
    let struct_id = ty.get_struct_id(ctx.env).expect("struct");
    let layout = ctx.get_struct_layout(&struct_id);

    // Check pointer equality of fields first.
    for field_offs in layout.field_order.iter().take(layout.pointer_count) {
        let (byte_offs, field_ty) = layout.offsets.get(field_offs).unwrap();

        emitln!(
            ctx.writer,
            "let f_x_{} := mload({})",
            field_offs,
            format!("add(x, {})", byte_offs)
        );

        emitln!(
            ctx.writer,
            "let f_y_{} := mload({})",
            field_offs,
            format!("add(y, {})", byte_offs)
        );
        let field_equality_call = format!(
            "{}(f_x_{}, f_y_{})",
            gen.equality_function(ctx, field_ty.clone()),
            field_offs,
            field_offs
        );
        emitln!(
            ctx.writer,
            "if {} {{\n  res:= false\n  leave\n}}",
            gen.call_builtin_str(
                ctx,
                YulFunction::LogicalNot,
                std::iter::once(field_equality_call)
            )
        );
    }

    // The remaining fields are all primitive. We directly check the memory content.
    if layout.pointer_count < layout.field_order.len() {
        let mut byte_offs = layout
            .offsets
            .get(&layout.field_order[layout.pointer_count])
            .unwrap()
            .0;
        assert_eq!(
            byte_offs % 32,
            0,
            "first non-pointer field on word boundary"
        );
        while byte_offs < layout.size {
            emitln!(
                ctx.writer,
                "if {} {{\n  res:= false\n  leave\n}}",
                gen.call_builtin_str(
                    ctx,
                    YulFunction::Neq,
                    vec![
                        format!("mload(add(x, {}))", byte_offs),
                        format!("mload(add(y, {}))", byte_offs)
                    ]
                    .into_iter()
                )
            );
            byte_offs += 32
        }
    }
    emitln!(ctx.writer, "res := true");
}
