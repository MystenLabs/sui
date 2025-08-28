// mod utils;

// #[cfg(test)]
// mod test {
//     use super::utils;
//     use move_compiler::Flags;
//     use revela::decompiler::Decompiler;

//     #[test]
//     fn decompile_builtin_libs() -> datatest_stable::Result<()> {
//         let mut src_scripts = vec![];
//         let mut src_modules = vec![];
//         let ref_output_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
//             .join("tests")
//             .join("refs");

//         let env_revela_stdlib_test_module_selector =
//             std::env::var("REVELA_STDLIB_TEST_MODULE_SELECTOR").ok();
//         let env_revela_stdlib_test_module_selector = env_revela_stdlib_test_module_selector
//             .as_ref()
//             .map(|s| s.split(",").collect::<Vec<_>>());
//         let env_revela_stdlib_test_module_selector =
//             env_revela_stdlib_test_module_selector.as_ref();

//         let env_revela_stdlib_test_output_only = std::env::var("REVELA_STDLIB_TEST_OUTPUT_ONLY")
//             .ok()
//             .is_some();

//         utils::tmp_project(vec![], |tmp_files| {
//             (src_scripts, src_modules) = utils::run_compiler(tmp_files, Flags::empty(), true);

//             assert_eq!(src_scripts.len(), 0, "Stdlib should have no script");

//             src_modules.iter().for_each(|module| {
//                 let module_id = module.self_id();
//                 let module_name = module_id.name().as_str();

//                 if env_revela_stdlib_test_module_selector.map(|s| s.contains(&module_name))
//                     == Some(false)
//                 {
//                     return;
//                 }

//                 println!("Decompiling {}", module_name);

//                 let module_vec = vec![module.clone()];

//                 let binaries = utils::into_binary_indexed_view(&module_vec);

//                 let mut decompiler = Decompiler::new(binaries, Default::default());
//                 let output = decompiler.decompile().expect("Unable to decompile");

//                 if env_revela_stdlib_test_output_only {
//                     println!("{}", output);
//                     return;
//                 }

//                 let output_path = ref_output_dir.join(format!("{}-decompiled.move", module_name));

//                 std::fs::write(&output_path, output).unwrap();
//             });
//         });

//         Ok(())
//     }
// }
