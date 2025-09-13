// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_model::symbol::Symbol;
use move_model_2::{model::Model, source_kind::SourceKind};

use std::{fs::File, path::Path};

fn generate_output<S: SourceKind>(input: Model<S>, output: Path) -> anyhow::Result<()> {
    let decompiled = crate::translate::model(input)?;

    let crate::ast::Decompiled { model, packages } = decompiled;

    for pkg in packages {
        // Ensure the package directory exists: output/pkg_name
        let pkg_dir = output.join(&pkg.name);
        std::fs::create_dir_all(&pkg_dir)?;

        // Iterate without moving the map/vec
        for (module_name, module) in &pkg.modules {
            let path = pkg_dir.join(format!("{module_name}.move"));

            // If generate_output returns a Result, use `?`; otherwise drop it
            generate_module(model, &path, &pkg.name, pkg.address, module)?;
        }
    }

    Ok(())
}

fn generate_module<S: SourceKind>(
    model: Model<S>,
    path: &_,
    name: Option<Symbol>,
    address: _,
    module: &crate::ast::Module,
) -> _ {
    let mut output = String::new();
    output.push_str(&format!("// Module: {}\n", module_name));
    output.push_str(&format!("// Address: {}\n\n", pkg.address));

    for func in module.functions {
        output.push_str(&format!("function {} {{\n", func.name));
        for stmt in func.body {
            output.push_str(&format!("    {}\n", stmt));
        }
        output.push_str("}\n\n");
    }

    write_to_file(
        &output,
        target.join(format!("{}_{}.move", pkg.name, module_name)),
    )?;
}
