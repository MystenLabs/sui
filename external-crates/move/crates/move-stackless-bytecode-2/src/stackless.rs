// TODO finish this file 
fn package<K: SourceKind>(ctx: &mut Context, package: Package<K>) -> anyhow::Result<()> {
    let package_name = package.name();
    let package_address = package.address();
    println!("\nPackage: {} ({})", package_name, package_address);

    for module in package.modules.values() {
        module(ctx, module)?;
    }
}

fn module<K: SourceKind>(ctx: &mut Context, module: Module<K>) -> anyhow::Result<()> {
    let module = module.compiled();
    let module_name = module.name();
    let module_address = module.address();
    println!("\nModule: {} ({})", module_name, module_address);

    for function in module.functions.values() {
        let function_name = &function.name;
        println!("\nFunction: {}", function_name);
        let code = function.code();
        for op in code {
            let instruction = bytecode(ctx, &op)?;
            ctx.ir_instructions.push(instruction);
        }
    }

    for instruction in &ctx.ir_instructions {
        match instruction {
            Instruction::Return(operands) => {
                println!("Return: {:?}", operands);
            }
            Instruction::Assign { lhs, rhs } => {
                println!("Assign: Var{:?} = {:?}", lhs, rhs);
            }
            _ => {
                // Handle other instructions as needed
                println!("Instruction: {:?}", instruction);
            }
        }
    }

    Ok(())
}
