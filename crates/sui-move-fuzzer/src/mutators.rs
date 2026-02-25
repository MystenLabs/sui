// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Targeted mutation strategies for the Sui Move VM fuzzer.
//!
//! Each mutation targets a specific known attack surface in the Move bytecode
//! verifier or Sui execution pipeline.

use arbitrary::Unstructured;
use move_binary_format::file_format::{
    AbilitySet, Bytecode, CodeUnit, DatatypeHandleIndex, EnumDefinition, EnumDefinitionIndex,
    FunctionDefinition, FunctionHandleIndex, FunctionInstantiation, JumpTableInner, SignatureIndex,
    StructDefInstantiationIndex, StructDefinitionIndex, VariantDefinition, VariantHandle,
};
use move_binary_format::CompiledModule;
use move_core_types::identifier::Identifier;

#[derive(Debug, Clone, Copy, arbitrary::Arbitrary)]
pub enum MutationKind {
    BoundsCorrupt,
    PassOrderViolation,
    RefSafetyStress,
    IdLeakLaunder,
    PrivateGenericsBypass,
    IntegerOverflowOffset,
    MeteringExhaust,
    SafeAssertDual,
    AbilityEscalation,
    EnumVariantConfuse,
}

/// Apply a targeted mutation to a CompiledModule.
pub fn apply_mutation(
    u: &mut Unstructured,
    module: &mut CompiledModule,
) -> arbitrary::Result<MutationKind> {
    let kind: MutationKind = u.arbitrary()?;
    match kind {
        MutationKind::BoundsCorrupt => mutate_bounds(u, module)?,
        MutationKind::PassOrderViolation => mutate_pass_order(u, module)?,
        MutationKind::RefSafetyStress => mutate_ref_safety(u, module)?,
        MutationKind::IdLeakLaunder => mutate_id_leak(u, module)?,
        MutationKind::PrivateGenericsBypass => mutate_private_generics(u, module)?,
        MutationKind::IntegerOverflowOffset => {} // no-op on CompiledModule, handled by apply_bytes_mutation
        MutationKind::MeteringExhaust => mutate_metering(u, module)?,
        MutationKind::SafeAssertDual => mutate_safe_assert(u, module)?,
        MutationKind::AbilityEscalation => mutate_ability_escalation(u, module)?,
        MutationKind::EnumVariantConfuse => mutate_enum_variant(u, module)?,
    }
    Ok(kind)
}

/// Apply a mutation to serialized module bytes (for IntegerOverflowOffset).
pub fn apply_bytes_mutation(u: &mut Unstructured, bytes: &mut [u8]) -> arbitrary::Result<()> {
    mutate_integer_overflow(u, bytes)
}

/// Pick a random boundary-adjacent value for a given table length.
fn boundary_value(u: &mut Unstructured, len: usize) -> arbitrary::Result<u16> {
    let len16 = len as u16;
    let choices: &[u16] = &[
        len16,
        len16.wrapping_sub(1),
        len16.wrapping_add(1),
        u16::MAX,
        0,
    ];
    let idx = u.choose_index(choices.len())?;
    Ok(choices[idx])
}

/// Get a mutable reference to a random function's code unit, if any exist.
fn pick_code_unit<'a>(
    u: &mut Unstructured,
    module: &'a mut CompiledModule,
) -> arbitrary::Result<Option<&'a mut CodeUnit>> {
    let fns_with_code: Vec<usize> = module
        .function_defs
        .iter()
        .enumerate()
        .filter(|(_, f)| f.code.is_some())
        .map(|(i, _)| i)
        .collect();
    if fns_with_code.is_empty() {
        return Ok(None);
    }
    let idx = *u.choose(&fns_with_code)?;
    Ok(module.function_defs[idx].code.as_mut())
}

// ---------------------------------------------------------------------------
// 1. BoundsCorrupt
// ---------------------------------------------------------------------------

fn mutate_bounds(u: &mut Unstructured, module: &mut CompiledModule) -> arbitrary::Result<()> {
    let variant: u8 = u.int_in_range(0..=3)?;
    match variant {
        0 => {
            // Corrupt function_handle parameters -> out of bounds SignatureIndex
            if let Some(fh) = module.function_handles.first_mut() {
                fh.parameters = SignatureIndex(boundary_value(u, module.signatures.len())?);
            }
        }
        1 => {
            // Corrupt struct_def struct_handle -> out of bounds DatatypeHandleIndex
            if let Some(sd) = module.struct_defs.first_mut() {
                sd.struct_handle =
                    DatatypeHandleIndex(boundary_value(u, module.datatype_handles.len())?);
            }
        }
        2 => {
            // Corrupt branch offset in code
            if let Some(code) = pick_code_unit(u, module)?
                && !code.code.is_empty()
            {
                let code_len = code.code.len();
                let target = boundary_value(u, code_len)?;
                let pos = u.choose_index(code_len)?;
                let bc_variant: u8 = u.int_in_range(0..=2)?;
                code.code[pos] = match bc_variant {
                    0 => Bytecode::Branch(target),
                    1 => Bytecode::BrTrue(target),
                    _ => Bytecode::BrFalse(target),
                };
            }
        }
        _ => {
            // Corrupt local index in CopyLoc/MoveLoc/StLoc
            if let Some(code) = pick_code_unit(u, module)?
                && !code.code.is_empty()
            {
                let code_len = code.code.len();
                let pos = u.choose_index(code_len)?;
                // Use u8::MAX as out-of-bounds local
                let local_idx: u8 = *u.choose(&[0, u8::MAX, u8::MAX - 1])?;
                let bc_variant: u8 = u.int_in_range(0..=2)?;
                code.code[pos] = match bc_variant {
                    0 => Bytecode::CopyLoc(local_idx),
                    1 => Bytecode::MoveLoc(local_idx),
                    _ => Bytecode::StLoc(local_idx),
                };
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 2. PassOrderViolation
// ---------------------------------------------------------------------------

fn mutate_pass_order(u: &mut Unstructured, module: &mut CompiledModule) -> arbitrary::Result<()> {
    let deprecated_opcodes: &[Bytecode] = &[
        Bytecode::MoveFromDeprecated(StructDefinitionIndex(0)),
        Bytecode::MoveToDeprecated(StructDefinitionIndex(0)),
        Bytecode::ImmBorrowGlobalDeprecated(StructDefinitionIndex(0)),
        Bytecode::MutBorrowGlobalDeprecated(StructDefinitionIndex(0)),
        Bytecode::ExistsDeprecated(StructDefinitionIndex(0)),
        Bytecode::MoveFromGenericDeprecated(StructDefInstantiationIndex(0)),
        Bytecode::MoveToGenericDeprecated(StructDefInstantiationIndex(0)),
        Bytecode::ImmBorrowGlobalGenericDeprecated(StructDefInstantiationIndex(0)),
        Bytecode::MutBorrowGlobalGenericDeprecated(StructDefInstantiationIndex(0)),
        Bytecode::ExistsGenericDeprecated(StructDefInstantiationIndex(0)),
    ];

    let opcode = u.choose(deprecated_opcodes)?.clone();

    if let Some(code) = pick_code_unit(u, module)? {
        if code.code.is_empty() {
            code.code.push(opcode);
        } else {
            let pos = u.choose_index(code.code.len() + 1)?;
            code.code.insert(pos, opcode);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 3. RefSafetyStress
// ---------------------------------------------------------------------------

fn mutate_ref_safety(u: &mut Unstructured, module: &mut CompiledModule) -> arbitrary::Result<()> {
    let pattern: u8 = u.int_in_range(0..=1)?;

    if let Some(code) = pick_code_unit(u, module)? {
        let code_len = code.code.len().max(1) as u16;
        match pattern {
            0 => {
                // MutBorrowLoc, BrTrue, Pop, Branch, FreezeRef, Pop
                let branch_target = u.int_in_range(0..=code_len.saturating_sub(1))?;
                let seq = vec![
                    Bytecode::MutBorrowLoc(0),
                    Bytecode::LdTrue,
                    Bytecode::BrTrue(branch_target),
                    Bytecode::Pop,
                    Bytecode::Branch(branch_target),
                    Bytecode::FreezeRef,
                    Bytecode::Pop,
                ];
                let pos = u.choose_index(code.code.len() + 1)?;
                for (i, bc) in seq.into_iter().enumerate() {
                    code.code.insert(pos + i, bc);
                }
            }
            _ => {
                // Interleave borrow patterns: MutBorrowLoc(0), ImmBorrowLoc(0) on same local
                let seq = vec![
                    Bytecode::MutBorrowLoc(0),
                    Bytecode::Pop,
                    Bytecode::ImmBorrowLoc(0),
                    Bytecode::Pop,
                    Bytecode::MutBorrowLoc(0),
                    Bytecode::FreezeRef,
                    Bytecode::Pop,
                ];
                for (i, bc) in seq.into_iter().enumerate() {
                    code.code.insert(i, bc);
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 4. IdLeakLaunder
// ---------------------------------------------------------------------------

fn mutate_id_leak(u: &mut Unstructured, module: &mut CompiledModule) -> arbitrary::Result<()> {
    let pattern: u8 = u.int_in_range(0..=1)?;

    // Read signatures length before borrowing module mutably via pick_code_unit.
    let sig_idx = if module.signatures.is_empty() {
        SignatureIndex(0)
    } else {
        SignatureIndex(u.int_in_range(0..=(module.signatures.len() as u16 - 1))?)
    };

    if let Some(code) = pick_code_unit(u, module)? {
        match pattern {
            0 => {
                // VecPack(sig, 1) + VecUnpack(sig, 1) wrapping a value
                let pos = u.choose_index(code.code.len().max(1))?;
                code.code.insert(pos, Bytecode::VecPack(sig_idx, 1));
                code.code.insert(pos + 1, Bytecode::VecUnpack(sig_idx, 1));
            }
            _ => {
                // StLoc(idx) + CopyLoc(idx) - attempt to copy instead of move
                let local_idx: u8 = u.int_in_range(0..=3)?;
                let pos = u.choose_index(code.code.len().max(1))?;
                code.code.insert(pos, Bytecode::StLoc(local_idx));
                code.code.insert(pos + 1, Bytecode::CopyLoc(local_idx));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 5. PrivateGenericsBypass
// ---------------------------------------------------------------------------

fn mutate_private_generics(
    u: &mut Unstructured,
    module: &mut CompiledModule,
) -> arbitrary::Result<()> {
    if module.function_handles.is_empty() || module.signatures.is_empty() {
        return Ok(());
    }

    // Pick a function handle (ideally from a foreign module)
    let fh_idx = u.choose_index(module.function_handles.len())?;

    // Find or create a signature with a foreign type argument
    let type_sig_idx = if module.datatype_handles.is_empty() {
        // No datatype handles, use an existing signature
        SignatureIndex(u.int_in_range(0..=(module.signatures.len() as u16 - 1))?)
    } else {
        // Pick a datatype handle and create a signature referencing it
        use move_binary_format::file_format::{Signature, SignatureToken};
        let dh_idx = u.choose_index(module.datatype_handles.len())?;
        let token = SignatureToken::Datatype(DatatypeHandleIndex(dh_idx as u16));
        let sig = Signature(vec![token]);
        let idx = module.signatures.len() as u16;
        module.signatures.push(sig);
        SignatureIndex(idx)
    };

    module.function_instantiations.push(FunctionInstantiation {
        handle: FunctionHandleIndex(fh_idx as u16),
        type_parameters: type_sig_idx,
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// 6. IntegerOverflowOffset (operates on bytes)
// ---------------------------------------------------------------------------

fn mutate_integer_overflow(u: &mut Unstructured, bytes: &mut [u8]) -> arbitrary::Result<()> {
    // The binary header is 9 bytes: 4 magic + 4 version + 1 table_count
    // Each table header is 9 bytes: 1 kind + 4 offset + 4 count
    if bytes.len() < 10 {
        return Ok(());
    }

    let table_count = bytes[8] as usize;
    let header_end = 9 + table_count * 9;

    let strategy: u8 = u.int_in_range(0..=2)?;
    match strategy {
        0 => {
            // Corrupt table count fields with large values
            for i in 0..table_count {
                let count_offset = 9 + i * 9 + 5; // skip kind(1) + offset(4), count starts at +5
                if count_offset + 4 <= bytes.len() {
                    let corrupt: u8 = u.int_in_range(0..=1)?;
                    let value: u32 = if corrupt == 0 {
                        u32::MAX
                    } else {
                        u16::MAX as u32
                    };
                    bytes[count_offset..count_offset + 4].copy_from_slice(&value.to_le_bytes());
                }
            }
        }
        1 => {
            // Corrupt table offsets to point beyond the binary
            for i in 0..table_count {
                let offset_pos = 9 + i * 9 + 1; // skip kind(1), offset starts at +1
                if offset_pos + 4 <= bytes.len() {
                    let value = u32::MAX;
                    bytes[offset_pos..offset_pos + 4].copy_from_slice(&value.to_le_bytes());
                }
            }
        }
        _ => {
            // Corrupt ULEB128 values in the table content area
            if header_end < bytes.len() {
                let target = u.int_in_range(header_end..=bytes.len() - 1)?;
                // Overwrite with a 5-byte ULEB128 encoding of a large value
                let remaining = bytes.len() - target;
                if remaining >= 5 {
                    // Encode u32::MAX as 5-byte ULEB128: 0xFF 0xFF 0xFF 0xFF 0x0F
                    bytes[target] = 0xFF;
                    if target + 1 < bytes.len() {
                        bytes[target + 1] = 0xFF;
                    }
                    if target + 2 < bytes.len() {
                        bytes[target + 2] = 0xFF;
                    }
                    if target + 3 < bytes.len() {
                        bytes[target + 3] = 0xFF;
                    }
                    if target + 4 < bytes.len() {
                        bytes[target + 4] = 0x0F;
                    }
                } else {
                    // Just corrupt single byte with high-bit continuation
                    bytes[target] = 0xFF;
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 7. MeteringExhaust
// ---------------------------------------------------------------------------

fn mutate_metering(u: &mut Unstructured, module: &mut CompiledModule) -> arbitrary::Result<()> {
    let num_blocks: u16 = u.int_in_range(10..=200)?;

    if let Some(code) = pick_code_unit(u, module)? {
        code.code.clear();
        // Generate N basic blocks, each with a back-edge to block 0
        // Each block: LdTrue, BrFalse(next), Branch(0)
        // Block offsets: block i starts at i*3
        for i in 0..num_blocks {
            let next_block_offset = (i + 1) * 3;
            code.code.push(Bytecode::LdTrue);
            code.code.push(Bytecode::BrFalse(next_block_offset));
            code.code.push(Bytecode::Branch(0)); // back edge
        }
        // Final block: Ret
        code.code.push(Bytecode::Ret);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 8. SafeAssertDual
// ---------------------------------------------------------------------------

fn mutate_safe_assert(u: &mut Unstructured, module: &mut CompiledModule) -> arbitrary::Result<()> {
    // Same as PassOrderViolation -- triggers safe_assert! in BoundsChecker
    // when deprecate_global_storage_ops=true
    mutate_pass_order(u, module)
}

// ---------------------------------------------------------------------------
// 9. AbilityEscalation
// ---------------------------------------------------------------------------

fn mutate_ability_escalation(
    _u: &mut Unstructured,
    module: &mut CompiledModule,
) -> arbitrary::Result<()> {
    // Set a random datatype_handle abilities to ALL (0xF = Copy|Drop|Store|Key)
    if let Some(dh) = module.datatype_handles.first_mut() {
        dh.abilities = AbilitySet::ALL;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 10. EnumVariantConfuse
// ---------------------------------------------------------------------------

fn mutate_enum_variant(u: &mut Unstructured, module: &mut CompiledModule) -> arbitrary::Result<()> {
    if !module.enum_defs.is_empty() {
        let variant_strategy: u8 = u.int_in_range(0..=2)?;
        match variant_strategy {
            0 => {
                // Corrupt VariantHandle variant tag to u16::MAX
                if let Some(vh) = module.variant_handles.first_mut() {
                    vh.variant = u16::MAX;
                }
            }
            1 => {
                // Corrupt jump table entries in code
                for func_def in &mut module.function_defs {
                    if let Some(ref mut code) = func_def.code {
                        for jt in &mut code.jump_tables {
                            let JumpTableInner::Full(ref mut offsets) = jt.jump_table;
                            for offset in offsets.iter_mut() {
                                *offset = u16::MAX;
                            }
                        }
                    }
                }
            }
            _ => {
                // Corrupt an enum def's variant list
                if let Some(ed) = module.enum_defs.first_mut() {
                    // Add extra bogus variants with out-of-bounds identifiers
                    let bogus_name_idx = module.identifiers.len() as u16;
                    ed.variants.push(VariantDefinition {
                        variant_name: move_binary_format::file_format::IdentifierIndex(
                            bogus_name_idx,
                        ),
                        fields: vec![],
                    });
                }
            }
        }
    } else {
        // No enums exist -- add a minimal enum definition with corrupted variant count
        if module.datatype_handles.is_empty() || module.identifiers.is_empty() {
            return Ok(());
        }

        // Add identifier for the variant name
        let variant_name_idx = module.identifiers.len() as u16;
        module
            .identifiers
            .push(Identifier::new("V0").expect("valid identifier"));

        let enum_handle_idx = DatatypeHandleIndex(0);
        module.enum_defs.push(EnumDefinition {
            enum_handle: enum_handle_idx,
            variants: vec![VariantDefinition {
                variant_name: move_binary_format::file_format::IdentifierIndex(variant_name_idx),
                fields: vec![],
            }],
        });

        // Add a variant handle with corrupted tag
        module.variant_handles.push(VariantHandle {
            enum_def: EnumDefinitionIndex(0),
            variant: u16::MAX,
        });
    }
    Ok(())
}

/// Ensure a module has at least one function with a code unit for mutation.
/// Returns true if a function was added or one already existed.
pub fn ensure_code_unit(module: &mut CompiledModule) -> bool {
    let has_code = module.function_defs.iter().any(|f| f.code.is_some());
    if has_code {
        return true;
    }

    // Need a function handle and a signature for the locals
    if module.function_handles.is_empty() || module.signatures.is_empty() {
        return false;
    }

    module.function_defs.push(FunctionDefinition {
        function: FunctionHandleIndex(0),
        visibility: move_binary_format::file_format::Visibility::Private,
        is_entry: false,
        acquires_global_resources: vec![],
        code: Some(CodeUnit {
            locals: SignatureIndex(0),
            code: vec![Bytecode::Ret],
            jump_tables: vec![],
        }),
    });
    true
}
