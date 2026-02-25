// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Grammar-based `CompiledModule` generator for Sui Move VM fuzzing.
//!
//! Generates structurally valid modules that pass bounds checking, duplication
//! checking, and type safety — reaching deep verifier passes (reference safety,
//! locals safety, Sui-specific checks). Uses `arbitrary::Unstructured` for
//! fuzzer-driven randomness so that libFuzzer can efficiently explore the
//! generation space.

use arbitrary::{Result, Unstructured};
use move_binary_format::file_format::{
    AbilitySet, Bytecode, CodeUnit, CompiledModule, DatatypeHandle, DatatypeHandleIndex,
    FieldDefinition, FieldHandle, FunctionDefinition, FunctionHandle, FunctionHandleIndex,
    IdentifierIndex, ModuleHandleIndex, Signature, SignatureIndex, SignatureToken,
    StructDefinition, StructDefinitionIndex, StructFieldInformation, TypeSignature, Visibility,
    empty_module,
};
use move_core_types::identifier::Identifier;

/// Configuration for module generation, derived from fuzzer input.
#[derive(Debug, Clone, arbitrary::Arbitrary)]
pub struct ModuleGenConfig {
    pub num_structs: u8,
    pub num_functions: u8,
    pub num_fields_per: u8,
    pub max_code_len: u8,
    pub has_key_struct: bool,
    pub has_entry_fn: bool,
}

impl ModuleGenConfig {
    fn clamped(&self) -> ClampedConfig {
        ClampedConfig {
            num_structs: (self.num_structs % 7) as usize,       // 0..=6
            num_functions: (self.num_functions % 6 + 1) as usize, // 1..=6
            num_fields_per: (self.num_fields_per % 5) as usize, // 0..=4
            max_code_len: (self.max_code_len % 45 + 4) as usize, // 4..=48
            has_entry_fn: self.has_entry_fn,
        }
    }
}

struct ClampedConfig {
    num_structs: usize,
    num_functions: usize,
    num_fields_per: usize,
    max_code_len: usize,
    has_entry_fn: bool,
}

/// Intern pool for deduplicating signatures.
struct SigPool {
    sigs: Vec<Signature>,
}

impl SigPool {
    fn new() -> Self {
        // Start with the empty signature that empty_module() provides
        Self {
            sigs: vec![Signature(vec![])],
        }
    }

    /// Return the index for this signature, adding it only if not already present.
    fn intern(&mut self, sig: Signature) -> SignatureIndex {
        for (i, existing) in self.sigs.iter().enumerate() {
            if existing.0 == sig.0 {
                return SignatureIndex(i as u16);
            }
        }
        let idx = self.sigs.len() as u16;
        self.sigs.push(sig);
        SignatureIndex(idx)
    }
}

/// Context passed to code generation with module-level struct/field information.
struct CodeGenContext {
    /// Types of all locals (params ++ locals).
    all_local_types: Vec<SignatureToken>,
    num_params: usize,
    return_types: Vec<SignatureToken>,
    /// struct_def_idx → vec of field types (in declaration order).
    struct_fields: Vec<Vec<SignatureToken>>,
    /// field_handle_idx → (struct_def_idx, field_idx, field_type).
    field_handle_info: Vec<(u16, u16, SignatureToken)>,
}

/// Builds a `CompiledModule` from fuzzer-driven configuration.
pub struct ModuleBuilder {
    config: ModuleGenConfig,
}

impl ModuleBuilder {
    pub fn new(config: ModuleGenConfig) -> Self {
        Self { config }
    }

    pub fn build(self, u: &mut Unstructured) -> Result<CompiledModule> {
        let cfg = self.config.clamped();
        let mut module = empty_module();
        let mut sig_pool = SigPool::new();

        // empty_module() gives us:
        //   identifiers[0] = "DUMMY"
        //   address_identifiers[0] = AccountAddress::ZERO
        //   module_handles[0] = { address: 0, name: 0 }
        //   signatures[0] = Signature(vec![])
        //   self_module_handle_idx = ModuleHandleIndex(0)

        // Step 1: Replace module name and add identifiers
        module.identifiers[0] = Identifier::new("fuzz_mod").unwrap();

        let struct_name_start = module.identifiers.len() as u16;
        for i in 0..cfg.num_structs {
            module
                .identifiers
                .push(Identifier::new(format!("S{i}")).unwrap());
        }

        let fn_name_start = module.identifiers.len() as u16;
        for i in 0..cfg.num_functions {
            module
                .identifiers
                .push(Identifier::new(format!("f{i}")).unwrap());
        }

        // Allocate at least 1 field name even if config says 0, since we
        // enforce a minimum of 1 field per struct to avoid ZERO_SIZED_STRUCT.
        let actual_field_names = cfg.num_fields_per.max(1);
        let field_name_start = module.identifiers.len() as u16;
        for i in 0..actual_field_names {
            module
                .identifiers
                .push(Identifier::new(format!("field{i}")).unwrap());
        }

        // "id" identifier reserved for UID field in Key structs (added by mutations)
        module
            .identifiers
            .push(Identifier::new("id").unwrap());

        // Step 2: Add struct definitions
        // We don't generate Key structs because Sui requires the first field to
        // be 0x2::object::UID, which requires importing the Sui framework.
        // Mutations can add Key ability to test the Sui verifier's UID check.
        for i in 0..cfg.num_structs {
            let abilities = AbilitySet::EMPTY
                | move_binary_format::file_format::Ability::Copy
                | move_binary_format::file_format::Ability::Drop
                | move_binary_format::file_format::Ability::Store;

            let dt_handle_idx = module.datatype_handles.len() as u16;
            module.datatype_handles.push(DatatypeHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(struct_name_start + i as u16),
                abilities,
                type_parameters: vec![],
            });

            let mut fields = Vec::new();

            // Ensure at least 1 field (Move rejects zero-sized structs)
            let field_count = cfg.num_fields_per.max(1);
            for f in 0..field_count {
                let sig_tok = pick_simple_type(u)?;
                let name_idx = field_name_start + f as u16;
                if (name_idx as usize) < module.identifiers.len() {
                    fields.push(FieldDefinition {
                        name: IdentifierIndex(name_idx),
                        signature: TypeSignature(sig_tok),
                    });
                }
            }

            module.struct_defs.push(StructDefinition {
                struct_handle: DatatypeHandleIndex(dt_handle_idx),
                field_information: if fields.is_empty() {
                    StructFieldInformation::Declared(vec![])
                } else {
                    StructFieldInformation::Declared(fields)
                },
            });
        }

        // Step 2b: Populate field_handles and build struct field type map
        let mut struct_fields: Vec<Vec<SignatureToken>> = Vec::new();
        let mut field_handle_info: Vec<(u16, u16, SignatureToken)> = Vec::new();
        for (struct_idx, struct_def) in module.struct_defs.iter().enumerate() {
            if let StructFieldInformation::Declared(fields) = &struct_def.field_information {
                let field_types: Vec<SignatureToken> =
                    fields.iter().map(|f| f.signature.0.clone()).collect();
                for (field_idx, fd) in fields.iter().enumerate() {
                    let fh_idx = module.field_handles.len();
                    module.field_handles.push(FieldHandle {
                        owner: StructDefinitionIndex(struct_idx as u16),
                        field: field_idx as u16,
                    });
                    field_handle_info.push((
                        struct_idx as u16,
                        field_idx as u16,
                        fd.signature.0.clone(),
                    ));
                    let _ = fh_idx; // used implicitly by index
                }
                struct_fields.push(field_types);
            } else {
                struct_fields.push(vec![]);
            }
        }

        // Step 3: Add function definitions with type-aware code generation
        for i in 0..cfg.num_functions {
            let is_entry = cfg.has_entry_fn && i == 0;

            let num_params: usize = *u.choose(&[0_usize, 1, 2])?;
            let num_returns: usize = *u.choose(&[0_usize, 0, 0, 1])?;

            let mut param_tokens = Vec::new();
            for _ in 0..num_params {
                param_tokens.push(pick_simple_type(u)?);
            }

            let mut return_tokens = Vec::new();
            for _ in 0..num_returns {
                return_tokens.push(pick_simple_type(u)?);
            }

            // Intern signatures to avoid duplicates
            let params_sig_idx = sig_pool.intern(Signature(param_tokens.clone()));
            let return_sig_idx = sig_pool.intern(Signature(return_tokens.clone()));

            let num_locals: usize = *u.choose(&[0_usize, 1, 2, 3])?;
            let mut local_tokens = Vec::new();
            for _ in 0..num_locals {
                // 25% chance of struct-typed local if structs exist
                if !struct_fields.is_empty() && u.ratio(1, 4)? {
                    let idx = u.int_in_range(0..=(struct_fields.len() - 1))?;
                    local_tokens
                        .push(SignatureToken::Datatype(DatatypeHandleIndex(idx as u16)));
                } else {
                    local_tokens.push(pick_simple_type(u)?);
                }
            }
            let locals_sig_idx = sig_pool.intern(Signature(local_tokens.clone()));

            // Build the full local type map: params ++ locals
            let mut all_local_types: Vec<SignatureToken> = param_tokens.clone();
            all_local_types.extend(local_tokens);

            let fn_handle_idx = module.function_handles.len() as u16;
            module.function_handles.push(FunctionHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(fn_name_start + i as u16),
                parameters: params_sig_idx,
                return_: return_sig_idx,
                type_parameters: vec![],
            });

            let ctx = CodeGenContext {
                all_local_types: all_local_types.clone(),
                num_params,
                return_types: return_tokens.clone(),
                struct_fields: struct_fields.clone(),
                field_handle_info: field_handle_info.clone(),
            };

            let code = gen_typed_code_unit(u, &ctx, cfg.max_code_len)?;

            module.function_defs.push(FunctionDefinition {
                function: FunctionHandleIndex(fn_handle_idx),
                visibility: Visibility::Public,
                is_entry,
                acquires_global_resources: vec![],
                code: Some(CodeUnit {
                    locals: locals_sig_idx,
                    code,
                    jump_tables: vec![],
                }),
            });
        }

        // Finalize: replace module's signature pool with deduplicated pool
        module.signatures = sig_pool.sigs;

        Ok(module)
    }
}

/// Pick a random primitive `SignatureToken`.
/// Excludes Address since we cannot produce Address constants via bytecode
/// (Address requires LdConst with a constant pool entry).
fn pick_simple_type(u: &mut Unstructured) -> Result<SignatureToken> {
    let choice: u8 = u.int_in_range(0..=6)?;
    Ok(match choice {
        0 => SignatureToken::Bool,
        1 => SignatureToken::U8,
        2 => SignatureToken::U16,
        3 => SignatureToken::U32,
        4 => SignatureToken::U64,
        5 => SignatureToken::U128,
        6 => SignatureToken::U256,
        _ => unreachable!(),
    })
}

/// Returns true if the token is an integer type (not Bool or Address).
fn is_integer(tok: &SignatureToken) -> bool {
    matches!(
        tok,
        SignatureToken::U8
            | SignatureToken::U16
            | SignatureToken::U32
            | SignatureToken::U64
            | SignatureToken::U128
            | SignatureToken::U256
    )
}

/// Returns true if the token is a primitive (non-struct, non-reference) type.
fn is_primitive(tok: &SignatureToken) -> bool {
    matches!(
        tok,
        SignatureToken::Bool
            | SignatureToken::U8
            | SignatureToken::U16
            | SignatureToken::U32
            | SignatureToken::U64
            | SignatureToken::U128
            | SignatureToken::U256
    )
}

/// Generate a type-safe bytecode sequence with references, branches, and struct ops.
///
/// Tracks a typed stack and local initialization state to ensure:
/// - All operations receive operands of the correct type
/// - CopyLoc/MoveLoc only access initialized locals
/// - StLoc stores the correct type to the local
/// - Return values match the function's return signature
/// - References are created and consumed in self-contained patterns
/// - Diamond branches push the same type on both sides
fn gen_typed_code_unit(
    u: &mut Unstructured,
    ctx: &CodeGenContext,
    max_len: usize,
) -> Result<Vec<Bytecode>> {
    let mut code = Vec::new();
    let mut type_stack: Vec<SignatureToken> = Vec::new();

    // Track which locals are initialized.
    // Parameters (indices 0..num_params) start initialized.
    let mut local_initialized = vec![false; ctx.all_local_types.len()];
    for init in local_initialized.iter_mut().take(ctx.num_params) {
        *init = true;
    }

    // Reserve space for return sequence + Ret
    let effective_max = max_len.saturating_sub(ctx.return_types.len() + 1).max(1);

    while code.len() < effective_max {
        let stack_len = type_stack.len();

        // --- Check what actions are possible ---

        let can_binop = stack_len >= 2 && {
            let a = &type_stack[stack_len - 1];
            let b = &type_stack[stack_len - 2];
            is_integer(a) && a == b
        };
        let can_shift = stack_len >= 2 && {
            let top = &type_stack[stack_len - 1];
            let below = &type_stack[stack_len - 2];
            *top == SignatureToken::U8 && is_integer(below)
        };
        let can_compare = stack_len >= 2 && {
            let a = &type_stack[stack_len - 1];
            let b = &type_stack[stack_len - 2];
            is_integer(a) && a == b
        };
        let can_not = stack_len >= 1 && type_stack[stack_len - 1] == SignatureToken::Bool;
        let can_cast = stack_len >= 1 && is_integer(&type_stack[stack_len - 1]);
        let can_pop = stack_len >= 1;

        // StLoc candidates: locals matching top-of-stack type
        let stloc_candidates: Vec<u8> = if can_pop {
            let top = &type_stack[stack_len - 1];
            ctx.all_local_types
                .iter()
                .enumerate()
                .filter(|(_, lt)| *lt == top)
                .map(|(i, _)| i as u8)
                .collect()
        } else {
            vec![]
        };

        // CopyLoc candidates: initialized locals (all our types have Copy)
        let copyloc_candidates: Vec<u8> = ctx
            .all_local_types
            .iter()
            .enumerate()
            .filter(|(i, lt)| local_initialized[*i] && is_primitive(lt))
            .map(|(i, _)| i as u8)
            .collect();

        // MoveLoc candidates: initialized locals with primitive types
        let moveloc_candidates: Vec<u8> = ctx
            .all_local_types
            .iter()
            .enumerate()
            .filter(|(i, lt)| local_initialized[*i] && is_primitive(lt))
            .map(|(i, _)| i as u8)
            .collect();

        // Borrow-read pattern: initialized primitive locals (net +1)
        let borrow_read_candidates: Vec<u8> = ctx
            .all_local_types
            .iter()
            .enumerate()
            .filter(|(i, lt)| local_initialized[*i] && is_primitive(lt))
            .map(|(i, _)| i as u8)
            .collect();

        // Borrow-write pattern: need matching primitive T on stack AND an initialized local of type T
        let borrow_write_candidates: Vec<u8> = if can_pop && is_primitive(&type_stack[stack_len - 1])
        {
            let top = &type_stack[stack_len - 1];
            ctx.all_local_types
                .iter()
                .enumerate()
                .filter(|(i, lt)| local_initialized[*i] && *lt == top)
                .map(|(i, _)| i as u8)
                .collect()
        } else {
            vec![]
        };

        // Borrow-field-read pattern: initialized struct-typed local
        let borrow_field_read_candidates: Vec<(u8, usize)> = ctx
            .all_local_types
            .iter()
            .enumerate()
            .filter_map(|(i, lt)| {
                if !local_initialized[i] {
                    return None;
                }
                if let SignatureToken::Datatype(dt_idx) = lt {
                    let sdi = dt_idx.0 as usize;
                    if sdi < ctx.struct_fields.len() && !ctx.struct_fields[sdi].is_empty() {
                        return Some((i as u8, sdi));
                    }
                }
                None
            })
            .collect();

        // If-diamond pattern: need Bool on top of stack, and enough room for
        // the branch overhead (BrFalse + then_const + Branch + else_const = 4+ insns)
        let can_if_diamond = stack_len >= 1
            && type_stack[stack_len - 1] == SignatureToken::Bool
            && code.len() + 6 < effective_max;

        // Pack: need a struct with fields we can produce constants for
        let pack_candidates: Vec<usize> = ctx
            .struct_fields
            .iter()
            .enumerate()
            .filter(|(_, fields)| {
                !fields.is_empty()
                    && fields.iter().all(is_primitive)
                    && code.len() + fields.len() + 1 < effective_max
            })
            .map(|(i, _)| i)
            .collect();

        // Unpack: need struct value on top of stack
        let can_unpack = can_pop && {
            if let SignatureToken::Datatype(dt_idx) = &type_stack[stack_len - 1] {
                let sdi = dt_idx.0 as usize;
                sdi < ctx.struct_fields.len() && !ctx.struct_fields[sdi].is_empty()
            } else {
                false
            }
        };

        // CopyLoc for struct-typed locals (all our structs have Copy)
        let copyloc_struct_candidates: Vec<u8> = ctx
            .all_local_types
            .iter()
            .enumerate()
            .filter(|(i, lt)| {
                local_initialized[*i] && matches!(lt, SignatureToken::Datatype(_))
            })
            .map(|(i, _)| i as u8)
            .collect();

        // --- Build weighted action list ---
        // Actions: 0=push_const 1=binop 2=compare 3=not 4=cast 5=pop 6=stloc
        //   7=copyloc 8=shift 9=moveloc 10=borrow_read 11=borrow_write
        //   12=borrow_field_read 13=if_diamond 14=pack 15=unpack
        let mut actions: Vec<u8> = Vec::new();

        // push_const: weight 3
        actions.extend_from_slice(&[0, 0, 0]);
        if can_binop {
            actions.push(1);
        }
        if can_compare {
            actions.push(2);
        }
        if can_not {
            actions.push(3);
        }
        if can_cast {
            actions.push(4);
        }
        if can_pop {
            actions.push(5);
        }
        if !stloc_candidates.is_empty() {
            actions.push(6);
        }
        if !copyloc_candidates.is_empty() {
            // weight 2
            actions.extend_from_slice(&[7, 7]);
        }
        if can_shift {
            actions.push(8);
        }
        if !moveloc_candidates.is_empty() {
            actions.push(9);
        }
        if !borrow_read_candidates.is_empty() {
            // weight 2
            actions.extend_from_slice(&[10, 10]);
        }
        if !borrow_write_candidates.is_empty() {
            actions.push(11);
        }
        if !borrow_field_read_candidates.is_empty() {
            actions.push(12);
        }
        if can_if_diamond {
            actions.push(13);
        }
        if !pack_candidates.is_empty() {
            actions.push(14);
        }
        if can_unpack {
            actions.push(15);
        }
        if !copyloc_struct_candidates.is_empty() {
            actions.push(7); // reuse copyloc action for struct locals
        }

        let action = *u.choose(&actions)?;
        match action {
            0 => {
                // Push a typed constant
                let tok = pick_simple_type(u)?;
                emit_typed_const(u, &mut code, &tok)?;
                type_stack.push(tok);
            }
            1 => {
                // Binary op: pop 2 same-type integers, push 1 of same type
                let result_type = type_stack[stack_len - 1].clone();
                type_stack.pop();
                type_stack.pop();
                emit_arith_binop(u, &mut code)?;
                type_stack.push(result_type);
            }
            2 => {
                // Comparison: pop 2 same-type integers, push Bool
                type_stack.pop();
                type_stack.pop();
                let cmp = u.choose(&[Bytecode::Lt, Bytecode::Gt, Bytecode::Le, Bytecode::Ge])?;
                code.push(cmp.clone());
                type_stack.push(SignatureToken::Bool);
            }
            3 => {
                // Not: pop Bool, push Bool
                code.push(Bytecode::Not);
            }
            4 => {
                // Cast: pop integer, push target integer type
                type_stack.pop();
                let (cast_bc, cast_type) = pick_cast(u)?;
                code.push(cast_bc);
                type_stack.push(cast_type);
            }
            5 => {
                // Pop
                type_stack.pop();
                code.push(Bytecode::Pop);
            }
            6 => {
                // StLoc
                let idx = *u.choose(&stloc_candidates)?;
                type_stack.pop();
                code.push(Bytecode::StLoc(idx));
                local_initialized[idx as usize] = true;
            }
            7 => {
                // CopyLoc (primitive or struct — all our types have Copy)
                let all_copy: Vec<u8> = ctx
                    .all_local_types
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| local_initialized[*i])
                    .map(|(i, _)| i as u8)
                    .collect();
                if let Ok(&idx) = u.choose(&all_copy) {
                    let ty = ctx.all_local_types[idx as usize].clone();
                    code.push(Bytecode::CopyLoc(idx));
                    type_stack.push(ty);
                } else {
                    // Fallback: push constant
                    let tok = pick_simple_type(u)?;
                    emit_typed_const(u, &mut code, &tok)?;
                    type_stack.push(tok);
                }
            }
            8 => {
                // Shift: pop U8 and integer, push integer
                let result_type = type_stack[stack_len - 2].clone();
                type_stack.pop();
                type_stack.pop();
                let op = u.choose(&[Bytecode::Shl, Bytecode::Shr])?;
                code.push(op.clone());
                type_stack.push(result_type);
            }
            9 => {
                // MoveLoc: moves value out of local, marks it unavailable
                let idx = *u.choose(&moveloc_candidates)?;
                let ty = ctx.all_local_types[idx as usize].clone();
                code.push(Bytecode::MoveLoc(idx));
                type_stack.push(ty);
                local_initialized[idx as usize] = false;
            }
            10 => {
                // Borrow-Freeze-Read pattern (net +1): BorrowLoc → FreezeRef → ReadRef
                let idx = *u.choose(&borrow_read_candidates)?;
                code.push(Bytecode::MutBorrowLoc(idx));
                code.push(Bytecode::FreezeRef);
                code.push(Bytecode::ReadRef);
                let ty = ctx.all_local_types[idx as usize].clone();
                type_stack.push(ty);
            }
            11 => {
                // Borrow-Write pattern (net -1): BorrowLoc → WriteRef
                let idx = *u.choose(&borrow_write_candidates)?;
                type_stack.pop(); // consume the T value on stack
                code.push(Bytecode::MutBorrowLoc(idx));
                code.push(Bytecode::WriteRef);
            }
            12 => {
                // Borrow-Field-Read pattern (net +1):
                //   BorrowLoc(struct_local) → ImmBorrowField(fh_idx) → ReadRef
                let &(local_idx, struct_def_idx) =
                    u.choose(&borrow_field_read_candidates)?;
                let fields = &ctx.struct_fields[struct_def_idx];
                let field_idx = u.int_in_range(0..=(fields.len() - 1))?;
                let field_type = fields[field_idx].clone();

                // Find the matching field_handle_index
                let fh_idx = ctx
                    .field_handle_info
                    .iter()
                    .position(|(sdi, fi, _)| {
                        *sdi == struct_def_idx as u16 && *fi == field_idx as u16
                    })
                    .unwrap();

                code.push(Bytecode::ImmBorrowLoc(local_idx));
                code.push(Bytecode::ImmBorrowField(
                    move_binary_format::file_format::FieldHandleIndex(fh_idx as u16),
                ));
                code.push(Bytecode::ReadRef);
                type_stack.push(field_type);
            }
            13 => {
                // If-then-else diamond (pops Bool, pushes T):
                //   BrFalse(else_offset)
                //   <then: emit_typed_const(T)>
                //   Branch(end_offset)
                //   <else: emit_typed_const(T)>
                //   <end>
                type_stack.pop(); // consume Bool

                let tok = pick_simple_type(u)?;

                // Emit BrFalse with placeholder offset
                let brfalse_idx = code.len();
                code.push(Bytecode::BrFalse(0));

                // Then branch: push constant of type T
                emit_typed_const(u, &mut code, &tok)?;

                // Branch to end with placeholder
                let branch_idx = code.len();
                code.push(Bytecode::Branch(0));

                // Else branch starts here
                let else_offset = code.len() as u16;
                emit_typed_const(u, &mut code, &tok)?;

                // End label
                let end_offset = code.len() as u16;

                // Backpatch
                code[brfalse_idx] = Bytecode::BrFalse(else_offset);
                code[branch_idx] = Bytecode::Branch(end_offset);

                type_stack.push(tok);
            }
            14 => {
                // Pack: push field constants then Pack
                let struct_idx = *u.choose(&pack_candidates)?;
                let fields = &ctx.struct_fields[struct_idx];
                for field_ty in fields {
                    emit_typed_const(u, &mut code, field_ty)?;
                }
                code.push(Bytecode::Pack(StructDefinitionIndex(struct_idx as u16)));
                type_stack.push(SignatureToken::Datatype(DatatypeHandleIndex(
                    struct_idx as u16,
                )));
            }
            15 => {
                // Unpack: pop struct, push field values
                let dt_idx = if let SignatureToken::Datatype(idx) = &type_stack[stack_len - 1] {
                    idx.0 as usize
                } else {
                    unreachable!()
                };
                type_stack.pop();
                let fields = ctx.struct_fields[dt_idx].clone();
                code.push(Bytecode::Unpack(StructDefinitionIndex(dt_idx as u16)));
                for ft in &fields {
                    type_stack.push(ft.clone());
                }
            }
            _ => unreachable!(),
        }
    }

    // Balance the stack: pop everything, then push return values as constants.
    while let Some(ty) = type_stack.pop() {
        // Struct values also have Drop, so Pop works for all our types
        let _ = ty;
        code.push(Bytecode::Pop);
    }

    for ret_ty in &ctx.return_types {
        emit_typed_const(u, &mut code, ret_ty)?;
    }

    code.push(Bytecode::Ret);
    Ok(code)
}

/// Emit a constant of a specific type onto the stack.
fn emit_typed_const(
    u: &mut Unstructured,
    code: &mut Vec<Bytecode>,
    tok: &SignatureToken,
) -> Result<()> {
    match tok {
        SignatureToken::Bool => {
            if u.arbitrary::<bool>()? {
                code.push(Bytecode::LdTrue);
            } else {
                code.push(Bytecode::LdFalse);
            }
        }
        SignatureToken::U8 => {
            code.push(Bytecode::LdU8(u.arbitrary()?));
        }
        SignatureToken::U16 => {
            code.push(Bytecode::LdU16(u.arbitrary()?));
        }
        SignatureToken::U32 => {
            code.push(Bytecode::LdU32(u.arbitrary()?));
        }
        SignatureToken::U64 => {
            code.push(Bytecode::LdU64(u.arbitrary()?));
        }
        SignatureToken::U128 => {
            code.push(Bytecode::LdU128(u.arbitrary()?));
        }
        SignatureToken::U256 => {
            code.push(Bytecode::LdU256(Box::new(move_core_types::u256::U256::from(
                u.int_in_range(0u64..=u64::MAX)?,
            ))));
        }
        _ => {
            // Fallback for Address or complex types — push U64 as placeholder.
            // pick_simple_type() excludes Address, so this is only hit by
            // mutations or future extensions.
            code.push(Bytecode::LdU64(0));
        }
    }
    Ok(())
}

/// Pick a random cast instruction and its result type.
fn pick_cast(u: &mut Unstructured) -> Result<(Bytecode, SignatureToken)> {
    let choice: u8 = u.int_in_range(0..=5)?;
    Ok(match choice {
        0 => (Bytecode::CastU8, SignatureToken::U8),
        1 => (Bytecode::CastU16, SignatureToken::U16),
        2 => (Bytecode::CastU32, SignatureToken::U32),
        3 => (Bytecode::CastU64, SignatureToken::U64),
        4 => (Bytecode::CastU128, SignatureToken::U128),
        5 => (Bytecode::CastU256, SignatureToken::U256),
        _ => unreachable!(),
    })
}

/// Emit an arithmetic binary operation (pop 2 same-type integers, push 1).
/// Shl/Shr are handled separately since they require U8 as second operand.
fn emit_arith_binop(u: &mut Unstructured, code: &mut Vec<Bytecode>) -> Result<()> {
    let op = u.choose(&[
        Bytecode::Add,
        Bytecode::Sub,
        Bytecode::Mul,
        Bytecode::Div,
        Bytecode::Mod,
        Bytecode::BitAnd,
        Bytecode::BitOr,
        Bytecode::Xor,
    ])?;
    code.push(op.clone());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_module_passes_bounds_check() {
        let data: Vec<u8> = (0..256).map(|i| (i % 256) as u8).collect();
        let mut u = Unstructured::new(&data);
        let config: ModuleGenConfig = u.arbitrary().unwrap();
        let builder = ModuleBuilder::new(config);
        let module = builder.build(&mut u).unwrap();

        let mut bytes = vec![];
        module.serialize(&mut bytes).unwrap();

        let config = move_binary_format::binary_config::BinaryConfig::standard();
        let result = CompiledModule::deserialize_with_config(&bytes, &config);
        assert!(result.is_ok(), "Bounds check failed: {result:?}");
    }

    #[test]
    fn generated_module_has_functions() {
        let data: Vec<u8> = (0..256).map(|i| ((i * 7) % 256) as u8).collect();
        let mut u = Unstructured::new(&data);
        let config: ModuleGenConfig = u.arbitrary().unwrap();
        let builder = ModuleBuilder::new(config);
        let module = builder.build(&mut u).unwrap();
        assert!(
            !module.function_defs.is_empty(),
            "Module must have at least one function"
        );
    }

    #[test]
    fn multiple_seeds_produce_valid_modules() {
        for seed in 0u8..20 {
            let data: Vec<u8> = (0..512)
                .map(|i| ((i as u16).wrapping_mul(seed as u16 + 1) % 256) as u8)
                .collect();
            let mut u = Unstructured::new(&data);
            let config: ModuleGenConfig = match u.arbitrary() {
                Ok(c) => c,
                Err(_) => continue,
            };
            let builder = ModuleBuilder::new(config);
            let module = match builder.build(&mut u) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let mut bytes = vec![];
            module.serialize(&mut bytes).unwrap();

            let bin_config = move_binary_format::binary_config::BinaryConfig::standard();
            let result = CompiledModule::deserialize_with_config(&bytes, &bin_config);
            assert!(
                result.is_ok(),
                "Seed {seed} failed bounds check: {result:?}"
            );
        }
    }

    #[test]
    fn verification_pass_rates() {
        use move_bytecode_verifier::verify_module_with_config_metered;
        use move_bytecode_verifier_meter::dummy::DummyMeter;
        use crate::sui_harness::{run_full_verification, sui_verifier_config};
        use std::collections::HashMap;

        let verifier_config = sui_verifier_config();
        let mut pass_bounds = 0u32;
        let mut pass_move = 0u32;
        let mut pass_sui = 0u32;
        let total = 200u32;
        let mut error_counts: HashMap<String, u32> = HashMap::new();

        for seed in 0..total {
            let data: Vec<u8> = (0..1024)
                .map(|i| {
                    ((i as u32)
                        .wrapping_mul(seed + 1)
                        .wrapping_add(seed.wrapping_mul(37))
                        % 256) as u8
                })
                .collect();
            let mut u = Unstructured::new(&data);
            let config: ModuleGenConfig = match u.arbitrary() {
                Ok(c) => c,
                Err(_) => continue,
            };
            let module = match ModuleBuilder::new(config).build(&mut u) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let mut bytes = vec![];
            if module.serialize(&mut bytes).is_err() {
                continue;
            }
            let bin_config = move_binary_format::binary_config::BinaryConfig::standard();
            if CompiledModule::deserialize_with_config(&bytes, &bin_config).is_ok() {
                pass_bounds += 1;
            }
            match verify_module_with_config_metered(&verifier_config, &module, &mut DummyMeter) {
                Ok(()) => pass_move += 1,
                Err(e) => {
                    let key = format!("Move: {:?}", e.major_status());
                    *error_counts.entry(key).or_insert(0) += 1;
                }
            }
            match run_full_verification(&module) {
                Ok(()) => pass_sui += 1,
                Err(e) => {
                    let key = format!("Sui: {e}");
                    *error_counts.entry(key).or_insert(0) += 1;
                }
            }
        }

        println!("Verification pass rates ({total} generated modules):");
        println!(
            "  Bounds check: {pass_bounds}/{total} ({:.0}%)",
            pass_bounds as f64 / total as f64 * 100.0
        );
        println!(
            "  Move verify:  {pass_move}/{total} ({:.0}%)",
            pass_move as f64 / total as f64 * 100.0
        );
        println!(
            "  Sui verify:   {pass_sui}/{total} ({:.0}%)",
            pass_sui as f64 / total as f64 * 100.0
        );
        println!("\nMove verifier error breakdown:");
        let mut errors: Vec<_> = error_counts.into_iter().collect();
        errors.sort_by(|a, b| b.1.cmp(&a.1));
        for (err, count) in &errors {
            println!("  {err}: {count}");
        }

        assert!(
            pass_bounds > total * 90 / 100,
            "Generator should produce >90% bounds-valid modules, got {pass_bounds}/{total}"
        );
    }
}
