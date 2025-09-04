// Copyright (c) Verichains, 2023

use std::collections::HashSet;

use anyhow::{Ok, Result};
use move_binary_format::file_format::{AbilitySet, DatatypeHandle};
use move_bytecode_source_map::source_map::SourceMap;

use move_ir_types::ast::ModuleName;
use move_model::{
    model::{FunctionEnv, GlobalEnv, ModuleEnv, StructEnv},
    ty::{PrimitiveType, Type},
};
use move_stackless_bytecode::{
    function_target::FunctionTarget,
    function_target_pipeline::{FunctionTargetPipeline, FunctionTargetsHolder, FunctionVariant},
};
use num::BigUint;

pub use self::reconstruct::OptimizerSettings;
use self::{
    model::{
        livevar_analysis::LiveVarAnalysisProcessor, peephole_analysis::PeepHoleProcessor,
        reaching_def_analysis::ReachingDefProcessor,
    },
    reconstruct::code_unit::SourceCodeUnit,
};

pub mod aptos_compat;
mod bin_to_compiler_translator;
mod cfg;
mod evaluator;
mod model;
mod naming;
mod reconstruct;
mod stackless_bytecode_display;
mod utils;

use self::aptos_compat::BinaryIndexedView;
use self::naming::Naming;

pub struct Decompiler<'a> {
    env: GlobalEnv,
    binaries: Vec<BinaryIndexedView<'a>>,
    optimizer_settings: OptimizerSettings,
}

impl<'a> Decompiler<'a> {
    pub fn new(
        binaries: Vec<BinaryIndexedView<'a>>,
        optimizer_settings: OptimizerSettings,
    ) -> Self {
        let env = GlobalEnv::new();
        Self {
            env,
            binaries,
            optimizer_settings,
        }
    }

    fn inline_decompile_type(
        &self,
        current_module: &ModuleEnv<'_>,
        ty: &Type,
        naming: &Naming,
    ) -> Result<String> {
        match ty {
            Type::Primitive(PrimitiveType::Bool) => Ok("bool".to_string()),
            Type::Primitive(PrimitiveType::U8) => Ok("u8".to_string()),
            Type::Primitive(PrimitiveType::U16) => Ok("u16".to_string()),
            Type::Primitive(PrimitiveType::U32) => Ok("u32".to_string()),
            Type::Primitive(PrimitiveType::U64) => Ok("u64".to_string()),
            Type::Primitive(PrimitiveType::U128) => Ok("u128".to_string()),
            Type::Primitive(PrimitiveType::U256) => Ok("u256".to_string()),
            Type::Primitive(PrimitiveType::Address) => Ok("address".to_string()),
            Type::Primitive(PrimitiveType::Signer) => Ok("signer".to_string()),

            // Types only appearing in specifications
            Type::Primitive(PrimitiveType::Num)
            | Type::Primitive(PrimitiveType::Range)
            | Type::Primitive(PrimitiveType::EventStore) => unreachable!(),

            Type::Tuple(tys) => {
                let ty_str = tys
                    .iter()
                    .map(|ty| self.inline_decompile_type(current_module, ty, naming))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                Ok(format!("({})", ty_str))
            }

            Type::Vector(ty) => {
                let ty_str = self.inline_decompile_type(current_module, ty, naming)?;
                Ok(format!("vector<{}>", ty_str))
            }

            Type::TypeParameter(idx) => Ok(naming.templated_type((*idx).into())),

            Type::Reference(false, ty) => {
                let ty_str = self.inline_decompile_type(current_module, ty, naming)?;
                Ok(format!("&{}", ty_str))
            }

            Type::Reference(true, ty) => {
                let ty_str = self.inline_decompile_type(current_module, ty, naming)?;
                Ok(format!("&mut {}", ty_str))
            }

            Type::Datatype(mid, sid, tys) => {
                let env = current_module.env;
                let module = env.get_module(*mid);
                let struct_env = module.get_struct(*sid);
                let struct_name = struct_env.get_name();
                let struct_name_display = struct_name.display(env.symbol_pool());
                let mut buf = String::new();

                buf.push_str(utils::shortest_prefix(current_module, mid).as_str());
                buf.push_str(struct_name_display.to_string().as_str());
                if !tys.is_empty() {
                    buf.push_str("<");
                    buf.push_str(
                        &tys.iter()
                            .map(|ty| self.inline_decompile_type(current_module, ty, naming))
                            .collect::<Result<Vec<_>>>()?
                            .join(", "),
                    );
                    buf.push_str(">");
                }
                Ok(buf)
            }

            // specification & temporary use only
            Type::Error => Ok("Error/*Failed to resolve*/".to_string()),

            Type::Fun(_, _)
            | Type::TypeDomain(_)
            | Type::ResourceDomain(_, _, _)
            | Type::Var(_) => {
                unreachable!("unexpected type")
            }
        }
    }

    fn decompile_abilityset(&self, s: AbilitySet, prefix: &str, join: &str) -> String {
        fn join_if(vec: &mut Vec<String>, condition: bool, value: &str) {
            if condition {
                vec.push(value.to_string());
            }
        }

        if s == AbilitySet::EMPTY {
            return String::new();
        }

        let mut res = Vec::new();
        join_if(&mut res, s.has_copy(), "copy");
        join_if(&mut res, s.has_drop(), "drop");
        join_if(&mut res, s.has_store(), "store");
        join_if(&mut res, s.has_key(), "key");

        format!("{}{}", prefix, res.join(join))
    }

    fn decompile_struct(
        &self,
        struct_bin: &DatatypeHandle,
        struct_env: &StructEnv<'_>,
        naming: &Naming,
    ) -> Result<SourceCodeUnit> {
        let mut res = SourceCodeUnit::new(0);

        let mut buf = String::new();
        buf.push_str("struct ");
        buf.push_str(
            struct_env
                .get_name()
                .display(struct_env.symbol_pool())
                .to_string()
                .as_str(),
        );

        let type_parameters = struct_env.get_type_parameters();
        if !type_parameters.is_empty() {
            buf.push_str("<");
            buf.push_str(
                type_parameters
                    .iter()
                    .zip(struct_bin.type_parameters.iter())
                    .enumerate()
                    .map(|(idx, (tp_from_env, tp_from_binary))| {
                        format!(
                            "{}{}{}",
                            // phantom information is not populated to struct_env
                            if tp_from_binary.is_phantom {
                                "phantom "
                            } else {
                                ""
                            },
                            naming.templated_type(idx),
                            self.decompile_abilityset(tp_from_env.1 .0, ": ", " + ")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
                    .as_str(),
            );
            buf.push_str(">");
        }

        let struct_ability = struct_env.get_abilities();
        if struct_ability != AbilitySet::EMPTY {
            buf.push_str(
                self.decompile_abilityset(struct_ability, " has ", ", ")
                    .as_str(),
            );
        }

        buf.push_str(" {");
        res.add_line(buf);

        let mut fields_block = SourceCodeUnit::new(1);
        for field in struct_env.get_fields() {
            let mut buf = String::new();
            buf.push_str(
                field
                    .get_name()
                    .display(struct_env.symbol_pool())
                    .to_string()
                    .as_str(),
            );
            buf.push_str(": ");
            buf.push_str(
                self.inline_decompile_type(&struct_env.module_env, &field.get_type(), naming)?
                    .as_str(),
            );
            buf.push_str(",");
            fields_block.add_line(buf);
        }

        res.add_block(fields_block);
        res.add_line("}".to_string());

        Ok(res)
    }

    fn decompile_function_header(
        &self,
        function_env: &FunctionEnv<'_>,
        naming: &Naming,
    ) -> Result<String> {
        let mut buf = String::new();

        if function_env.is_native() {
            buf.push_str("native ");
        }

        buf.push_str(function_env.visibility_str());

        if function_env.is_entry() {
            buf.push_str("entry ");
        }

        buf.push_str("fun ");

        buf.push_str(
            function_env
                .get_name()
                .display(function_env.symbol_pool())
                .to_string()
                .as_str(),
        );

        if function_env.get_type_parameter_count() > 0 {
            buf.push_str("<");
            buf.push_str(
                function_env
                    .get_type_parameters()
                    .iter()
                    .enumerate()
                    .map(|(idx, x)| {
                        format!(
                            "{}{}{}",
                            // TODO: if x.1.is_phantom { "phantom " } else { "" },
                            "",
                            naming.templated_type(idx),
                            self.decompile_abilityset(x.1 .0, ": ", " + ")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
                    .as_str(),
            );
            buf.push_str(">");
        }

        buf.push_str("(");
        buf.push_str(
            function_env
                .get_parameters()
                .iter()
                .enumerate()
                .map(|(idx, x)| {
                    format!(
                        "{}: {}",
                        naming.argument(idx),
                        self.inline_decompile_type(&function_env.module_env, &x.1, &naming)
                            .unwrap()
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
                .as_str(),
        );
        buf.push_str(")");

        if function_env.get_return_count() > 0 {
            buf.push_str(" : ");
            if function_env.get_return_count() > 1 {
                buf.push_str(
                    self.inline_decompile_type(
                        &function_env.module_env,
                        &Type::Tuple(function_env.get_return_types()),
                        naming,
                    )
                    .unwrap()
                    .as_str(),
                );
            } else {
                buf.push_str(
                    self.inline_decompile_type(
                        &function_env.module_env,
                        &function_env.get_return_type(0),
                        naming,
                    )
                    .unwrap()
                    .as_str(),
                );
            }
        }

        let resources = function_env.get_acquires_global_resources();
        if !resources.is_empty() {
            buf.push_str(" acquires ");
            buf.push_str(
                resources
                    .iter()
                    .map(|x| {
                        let module_env = &function_env.module_env;
                        let struct_env = module_env.get_struct(*x);
                        struct_env
                            .get_name()
                            .display(module_env.symbol_pool())
                            .to_string()
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
                    .as_str(),
            );
        }

        Ok(buf)
    }

    fn module_for_binary(&self, binary: &BinaryIndexedView) -> ModuleEnv<'_> {
        match binary {
            BinaryIndexedView::Module(compiled) => {
                let this_module_name = compiled.module_handle_at(compiled.self_handle_idx()).name;
                let this_module_name = compiled
                    .identifier_at(this_module_name)
                    .as_str()
                    .to_string();

                let this_module_addr = BigUint::from_bytes_be(&compiled.address().into_bytes());

                self.env
                    .get_modules()
                    .find(|m| {
                        let name = self.env.symbol_pool().string(m.get_name().name());
                        *name == this_module_name && m.get_name().addr() == &this_module_addr
                    })
                    .ok_or(anyhow::Error::msg(format!(
                        "module {} not found (impossible)",
                        this_module_name
                    )))
                    .unwrap()
            }
        }
    }

    pub fn decompile(&mut self) -> Result<String> {
        let mut pipeline = FunctionTargetPipeline::default();
        pipeline.set_max_loop(32);
        pipeline.add_processor(PeepHoleProcessor::new(32));
        pipeline.add_processor(ReachingDefProcessor::new());
        pipeline.add_processor(LiveVarAnalysisProcessor::new());

        let naming = Naming::new();

        let (program, module_map) =
            bin_to_compiler_translator::create_program(&self.binaries, &naming).unwrap();
        model::revela_helper::run_stackless_compiler(&mut self.env, program, module_map);

        // all module must be populated before decompiling
        for binary in &self.binaries {
            match binary {
                BinaryIndexedView::Module(compiled) => self.env.attach_compiled_module(
                    self.module_for_binary(&binary).get_id(),
                    (*compiled).clone(),
                    SourceMap::new(
                        bin_to_compiler_translator::fake_loc(),
                        move_ir_types::ast::ModuleIdent::new(
                            ModuleName(move_symbol_pool::Symbol::from("dummy")),
                            move_core_types::account_address::AccountAddress::ZERO,
                        ),
                    ),
                ),
            };
        }

        // Ignore the called functions detected by the pipeline, decompiler won't use it.
        for module in self.env.module_data.iter_mut() {
            for fun_data in module.function_data.values_mut() {
                *fun_data.called_funs.borrow_mut() = Some(Default::default());
            }
        }

        let mut result = SourceCodeUnit::new(0);

        // decompile
        for binary in self.binaries.clone() {
            let binary_module = binary.module();
            let module = self.module_for_binary(&binary);
            let version = binary.version();

            let mut targets = FunctionTargetsHolder::default();
            for f in module.get_functions() {
                targets.add_target(&f);
            }

            pipeline.run(&self.env, &mut targets);
            result.add_line(format!(
                "module {} {{",
                module.get_name().display_full(self.env.symbol_pool())
            ));

            let naming = naming.with_type_display(|t, naming| {
                self.inline_decompile_type(&module, t, naming).unwrap()
            });

            if !binary_module.struct_defs().is_empty() {
                let defs = binary_module.struct_defs();
                for idx in 0..defs.len() {
                    let s_idx = move_binary_format::file_format::StructDefinitionIndex(idx as u16);
                    let s = module.get_struct_by_def_idx(s_idx);
                    let s_bin = binary_module
                        .datatype_handle_at(binary_module.struct_def_at(s_idx).struct_handle);
                    let mut unit = self.decompile_struct(&s_bin, &s, &naming)?;
                    unit.add_line("".to_string());
                    unit.add_indent(1);
                    result.add_block(unit);
                }
            }

            let env_revela_function_selector = std::env::var("REVELA_FUNCTION_SELECTOR").ok();
            let env_revela_function_selector = env_revela_function_selector
                .as_ref()
                .map(|s| s.split(",").collect::<Vec<_>>());
            let env_revela_function_selector = env_revela_function_selector.as_ref();
            let env_revela_show_stackless_raw = std::env::var("REVELA_SHOW_STACKLESS_RAW").is_ok();

            let env_revela_show_stackless_decompiled =
                std::env::var("REVELA_SHOW_STACKLESS_DECOMPILED").is_ok();

            for f in module.get_functions() {
                if env_revela_function_selector
                    .map(|x| !x.contains(&f.get_name_str().as_str()))
                    .unwrap_or(false)
                {
                    continue;
                }
                let mut func_unit = SourceCodeUnit::new(1);
                let f_sig = self.decompile_function_header(&f, &naming)?;
                if f.is_native() {
                    func_unit.add_line(format!("{};", f_sig));
                } else {
                    func_unit.add_line(format!("{} {{", f_sig));

                    let function_target: FunctionTarget<'_> =
                        targets.get_target(&f, &FunctionVariant::Baseline);

                    if env_revela_show_stackless_raw {
                        let bytecode = function_target.get_bytecode();
                        let mut code_unit = SourceCodeUnit::new(1);
                        code_unit.add_line(format!("// Raw stackless bytecode"));
                        let label_offsets = Default::default();
                        for bytecode in bytecode.iter() {
                            func_unit.add_line(format!(
                                "// {:}",
                                bytecode.display(&function_target, &label_offsets)
                            ));
                        }
                        code_unit.add_line(format!("// End raw stackless bytecode"));
                        func_unit.add_block(code_unit);
                    }

                    let mut defined_vars = HashSet::new();
                    for idx in 0..function_target.get_parameter_count() {
                        defined_vars.insert(idx);
                    }

                    let mut cfg_decompiled =
                        cfg::stackless::decompile(function_target.get_bytecode(), &defined_vars)?;

                    // much of data from function_target should not be used because
                    // cfg_decompiled changed the bytecodes.
                    // variables offsets are still kept

                    if env_revela_show_stackless_decompiled {
                        let mut stackless_bytecode_display_ctx =
                            stackless_bytecode_display::StacklessBycodeDisplayContext::new(
                                &function_target,
                            );
                        cfg_decompiled.display(&mut stackless_bytecode_display_ctx);
                        let mut code_unit = SourceCodeUnit::new(1);
                        code_unit.add_line(format!("// Bytecode"));
                        for line in stackless_bytecode_display_ctx.result().split("\n") {
                            code_unit.add_line(format!("// {}", line));
                        }
                        code_unit.add_line(format!("// End Bytecode"));
                        func_unit.add_block(code_unit);
                    }

                    let mut sgen = reconstruct::SourceGen::new(
                        &mut cfg_decompiled,
                        &f,
                        &function_target,
                        &naming,
                    );

                    let mut code_unit = sgen.generate(&self.optimizer_settings)?;

                    code_unit.add_indent(1);
                    func_unit.add_block(code_unit);
                    func_unit.add_line("}".to_string());
                    func_unit.add_line("".to_string());
                }

                result.add_block(func_unit);
            }

            let mut footer = SourceCodeUnit::new(1);
            footer.add_line(format!("// decompiled from Move bytecode v{}", version));

            result.add_block(footer);
            result.add_line("}".to_string());
        }

        Ok(result.to_string())
    }
}
