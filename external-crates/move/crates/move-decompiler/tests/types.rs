mod utils;

#[cfg(test)]
mod test {
    use std::{collections::HashSet, env, path::PathBuf};

    use super::utils;
    use move_binary_format::{
        file_format::{
            FunctionDefinition, SignatureToken, StructDefinition, StructFieldInformation,
        },
        CompiledModule,
    };
    use move_compiler::Flags;
    use revela::decompiler::{Decompiler, OptimizerSettings};

    fn roundtrip_test(path: &str) {
        let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

        println!("manifest_dir: {}", manifest_dir);

        let src_modules = utils::run_compiler(
            vec![PathBuf::from(manifest_dir).join(path).to_str().unwrap()],
            Flags::empty(),
            false,
        );

        let binaries = utils::into_binary_indexed_view(&src_modules);

        let mut decompiler = Decompiler::new(binaries, Default::default());

        let output = decompiler.decompile().expect("Unable to decompile");

        println!("{}", output);

        utils::tmp_project(vec![(path, output.as_str())], |tmp_files| {
            let modules = utils::run_compiler(tmp_files, Flags::empty(), false);

            modules_should_match(&src_modules, &modules);

            let binaries = utils::into_binary_indexed_view(&modules);

            let mut decompiler = Decompiler::new(
                binaries,
                OptimizerSettings {
                    disable_optimize_variables_declaration: true,
                },
            );
            let output2 = decompiler.decompile().expect("Unable to decompile");

            // normalize by replace
            assert_eq!(output, output2);
        })
    }

    #[test]
    fn structs() {
        roundtrip_test("tests/typing/types.move");
    }

    #[test]
    fn function_signatures() {
        roundtrip_test("tests/typing/functions.move");
    }

    fn modules_should_match(src_modules: &[CompiledModule], modules: &[CompiledModule]) {
        fn sorted(m: &[CompiledModule]) -> Vec<((String, String), &CompiledModule)> {
            let mut v = m
                .iter()
                .map(|x| {
                    let self_id = x.self_id();
                    let address = self_id.address().to_hex_literal();
                    let name = self_id.name().to_string();
                    ((address, name), x)
                })
                .collect::<Vec<_>>();
            v.sort_by_key(|x| (x.0 .0.clone(), x.0 .1.clone()));
            v
        }

        assert_eq!(src_modules.len(), modules.len());
        let src_modules = sorted(src_modules);
        let modules = sorted(modules);

        let mut src_iter = src_modules.iter().peekable();
        let mut iter = modules.iter().peekable();

        while src_iter.peek().is_some() && iter.peek().is_some() {
            let src = src_iter.next().unwrap();

            let m = iter.next().unwrap();
            if src.0 < m.0 {
                panic!("Source module {}:{} is missing", src.0 .0, src.0 .1);
            }

            if src.0 > m.0 {
                panic!("Module {}:{} is missing", m.0 .0, m.0 .1);
            }

            module_should_match(src.1, m.1);
        }

        if src_iter.peek().is_some() {
            panic!(
                "Source module {}:{} is missing",
                src_iter.peek().unwrap().0 .0,
                src_iter.peek().unwrap().0 .1
            );
        }

        if iter.peek().is_some() {
            panic!(
                "Module {}:{} is missing",
                iter.peek().unwrap().0 .0,
                iter.peek().unwrap().0 .1
            );
        }
    }

    fn module_should_match(src: &CompiledModule, m: &CompiledModule) {
        let mut checked_structs = HashSet::new();

        // check structs
        for s in &src.struct_defs {
            let struct_handle = src.datatype_handle_at(s.struct_handle);
            let name = src.identifier_at(struct_handle.name).to_string();
            assert!(
                !checked_structs.contains(&name),
                "struct {} is duplicated???",
                name
            );

            let ms = m.struct_defs.iter().find(|x| {
                m.identifier_at(m.datatype_handle_at(x.struct_handle).name)
                    .to_string()
                    == name
            });

            assert!(ms.is_some(), "struct {} is not in target module", name);
            let ms = ms.unwrap();
            checked_structs.insert(name);
            struct_should_match(src, s, m, ms);
        }

        for s in &m.struct_defs {
            let struct_handle = m.datatype_handle_at(s.struct_handle);
            let name = m.identifier_at(struct_handle.name).to_string();
            assert!(
                checked_structs.contains(&name),
                "struct {} is not in source module",
                name
            );
        }

        // check declared functions signatures
        let mut checked_functions = HashSet::new();

        for f in &src.function_defs {
            let function_handle = src.function_handle_at(f.function);
            let name = src.identifier_at(function_handle.name).to_string();
            assert!(
                !checked_functions.contains(&name),
                "function {} is duplicated???",
                name
            );

            let mf = m.function_defs.iter().find(|x| {
                m.identifier_at(m.function_handle_at(x.function).name)
                    .to_string()
                    == name
            });

            assert!(mf.is_some(), "function {} is not in target module", name);
            let mf = mf.unwrap();
            checked_functions.insert(name);
            function_should_match(src, f, m, mf);
        }

        for f in &m.function_defs {
            let function_handle = m.function_handle_at(f.function);
            let name = m.identifier_at(function_handle.name).to_string();

            assert!(
                checked_functions.contains(&name),
                "function {} is not in source module",
                name
            );
        }
    }

    fn function_should_match(
        src: &CompiledModule,
        f: &FunctionDefinition,
        m: &CompiledModule,
        mf: &FunctionDefinition,
    ) {
        let src_function_handle = src.function_handle_at(f.function);
        let m_function_handle = m.function_handle_at(mf.function);

        let src_name = src.identifier_at(src_function_handle.name).to_string();
        let m_name = m.identifier_at(m_function_handle.name).to_string();

        assert_eq!(f.visibility, mf.visibility, "function visibility mismatch");
        assert_eq!(src_name, m_name, "function name mismatch");

        signature_should_match(
            src,
            src.signature_at(src_function_handle.parameters),
            m,
            m.signature_at(m_function_handle.parameters),
        );

        signature_should_match(
            src,
            src.signature_at(src_function_handle.return_),
            m,
            m.signature_at(m_function_handle.return_),
        );

        assert_eq!(
            src_function_handle.type_parameters.len(),
            m_function_handle.type_parameters.len(),
            "function type parameters count mismatch"
        );

        for (src_type_parameter, m_type_parameter) in src_function_handle
            .type_parameters
            .iter()
            .zip(m_function_handle.type_parameters.iter())
        {
            assert_eq!(
                src_type_parameter, m_type_parameter,
                "function type parameter mismatch"
            );
        }
    }

    fn signature_should_match(
        src: &CompiledModule,
        src_parameters: &move_binary_format::file_format::Signature,
        m: &CompiledModule,
        m_parameters: &move_binary_format::file_format::Signature,
    ) {
        assert_eq!(
            src_parameters.0.len(),
            m_parameters.0.len(),
            "signature parameters count mismatch"
        );

        for (src_parameter, parameter) in src_parameters.0.iter().zip(m_parameters.0.iter()) {
            signature_token_should_match(src, src_parameter, m, parameter);
        }
    }

    fn struct_should_match(
        src: &CompiledModule,
        src_struct_def: &StructDefinition,
        m: &CompiledModule,
        m_struct_def: &StructDefinition,
    ) {
        let src_struct_handle = src.datatype_handle_at(src_struct_def.struct_handle);
        let m_struct_handle = m.datatype_handle_at(m_struct_def.struct_handle);

        let src_name = src.identifier_at(src_struct_handle.name).to_string();
        let m_name = m.identifier_at(m_struct_handle.name).to_string();

        assert_eq!(src_name, m_name, "struct name mismatch");

        assert_eq!(
            src_struct_handle.abilities, m_struct_handle.abilities,
            "struct abilities mismatch"
        );

        assert_eq!(
            src_struct_handle.type_parameters.len(),
            m_struct_handle.type_parameters.len(),
            "struct type parameters count mismatch"
        );

        for (src_type_parameter, m_type_parameter) in src_struct_handle
            .type_parameters
            .iter()
            .zip(m_struct_handle.type_parameters.iter())
        {
            assert_eq!(
                src_type_parameter, m_type_parameter,
                "struct type parameter mismatch"
            );
        }

        let src_struct_is_native = matches!(
            src_struct_def.field_information,
            StructFieldInformation::Native
        );

        let m_struct_is_native = matches!(
            m_struct_def.field_information,
            StructFieldInformation::Native
        );

        assert_eq!(
            src_struct_is_native, m_struct_is_native,
            "struct is_native mismatch"
        );

        if src_struct_is_native {
            return;
        }

        let StructFieldInformation::Declared(src_fields) = &src_struct_def.field_information else {
            unreachable!()
        };

        let StructFieldInformation::Declared(fields) = &m_struct_def.field_information else {
            unreachable!()
        };

        assert_eq!(
            src_fields.len(),
            fields.len(),
            "struct field count mismatch"
        );

        for (src_field, field) in src_fields.iter().zip(fields.iter()) {
            assert_eq!(
                src.identifier_at(src_field.name).to_string(),
                src.identifier_at(field.name).to_string(),
                "struct field name mismatch"
            );

            signature_token_should_match(src, &src_field.signature.0, m, &field.signature.0);
        }
    }

    fn signature_token_should_match(
        src: &CompiledModule,
        src_signature: &SignatureToken,
        m: &CompiledModule,
        m_signature: &SignatureToken,
    ) {
        use move_binary_format::file_format::SignatureToken as ST;
        match src_signature {
            ST::Bool
            | ST::U8
            | ST::U16
            | ST::U32
            | ST::U64
            | ST::U128
            | ST::U256
            | ST::Address
            | ST::Signer => {
                assert_eq!(src_signature, m_signature);
            }

            ST::Vector(s) => {
                let ST::Vector(s2) = m_signature else {
                    panic!("signature mismatch");
                };
                signature_token_should_match(src, s.as_ref(), m, s2.as_ref());
            }

            ST::Datatype(s) => {
                let ST::Datatype(s2) = m_signature else {
                    panic!("signature mismatch");
                };
                let src_struct_handle = src.datatype_handle_at(*s);
                let m_struct_handle = m.datatype_handle_at(*s2);
                let src_name = src.identifier_at(src_struct_handle.name).to_string();
                let m_name = m.identifier_at(m_struct_handle.name).to_string();
                assert_eq!(src_name, m_name, "struct name mismatch");
            }

            ST::DatatypeInstantiation(di) => {
                let (s, sp) = di.as_ref();
                let ST::DatatypeInstantiation(di2) = m_signature else {
                    panic!("signature mismatch");
                };
                let (s2, sp2) = di2.as_ref();
                let src_struct_handle = src.datatype_handle_at(*s);
                let m_struct_handle = m.datatype_handle_at(*s2);
                let src_name = src.identifier_at(src_struct_handle.name).to_string();
                let m_name = m.identifier_at(m_struct_handle.name).to_string();
                assert_eq!(src_name, m_name, "struct name mismatch");
                assert_eq!(sp.len(), sp2.len(), "struct type parameters count mismatch");
                for (src_type_parameter, m_type_parameter) in sp.iter().zip(sp2.iter()) {
                    signature_token_should_match(src, src_type_parameter, m, m_type_parameter);
                }
            }

            ST::Reference(s) => {
                let ST::Reference(s2) = m_signature else {
                    panic!("signature mismatch");
                };
                signature_token_should_match(src, s.as_ref(), m, s2.as_ref());
            }

            ST::MutableReference(s) => {
                let ST::MutableReference(s2) = m_signature else {
                    panic!("signature mismatch");
                };
                signature_token_should_match(src, s.as_ref(), m, s2.as_ref());
            }

            ST::TypeParameter(idx) => {
                let ST::TypeParameter(idx2) = m_signature else {
                    panic!("signature mismatch");
                };
                assert_eq!(idx, idx2, "type parameter idx mismatch");
            }
        }
    }
}
