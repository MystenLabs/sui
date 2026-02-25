// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! High-level `ModuleSpec` format and `CompiledModule` builder for LLM-guided fuzzing.
//!
//! The LLM outputs a `ModuleSpec` JSON describing a module at a high level.
//! The builder converts it to a `CompiledModule` while managing all the
//! low-level index arithmetic (identifier pools, signature deduplication,
//! handle tables). This lets the LLM reason about structure and semantics
//! without worrying about Move binary format internals.
//!
//! # ModuleSpec JSON example
//!
//! ```json
//! {
//!   "module_name": "attack",
//!   "imports": [
//!     {"address": "0x2", "module": "object", "types": ["UID"], "functions": []}
//!   ],
//!   "structs": [
//!     {
//!       "name": "MyObj",
//!       "abilities": ["key", "store"],
//!       "fields": [
//!         {"name": "id", "type": "0x2::object::UID"},
//!         {"name": "val", "type": "u64"}
//!       ]
//!     }
//!   ],
//!   "functions": [
//!     {
//!       "name": "run",
//!       "visibility": "public",
//!       "is_entry": true,
//!       "parameters": ["u64"],
//!       "returns": [],
//!       "locals": ["u64"],
//!       "code": ["CopyLoc(0)", "LdU64(1)", "Add", "Pop", "Ret"]
//!     }
//!   ]
//! }
//! ```

use std::collections::HashMap;

use serde::Deserialize;

use move_binary_format::file_format::{
    Ability, AbilitySet, AddressIdentifierIndex, Bytecode, CodeUnit, CompiledModule,
    DatatypeHandle, DatatypeHandleIndex, FieldDefinition, FieldHandle, FieldHandleIndex,
    FunctionDefinition, FunctionHandle, FunctionHandleIndex, IdentifierIndex, ModuleHandle,
    ModuleHandleIndex, Signature, SignatureIndex, SignatureToken, StructDefinition,
    StructDefinitionIndex, StructFieldInformation, TypeSignature, Visibility, empty_module,
};
use move_core_types::account_address::AccountAddress;
use move_core_types::identifier::Identifier;

// ─── Spec types (LLM output format) ──────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleSpec {
    pub module_name: String,
    #[serde(default)]
    pub imports: Vec<ImportSpec>,
    #[serde(default)]
    pub structs: Vec<StructSpec>,
    pub functions: Vec<FunctionSpec>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImportSpec {
    /// Hex address, e.g. "0x2" or full 32-byte form.
    pub address: String,
    pub module: String,
    /// Type names exported by this module that we want to reference.
    #[serde(default)]
    pub types: Vec<String>,
    /// Function names exported by this module that we want to Call.
    #[serde(default)]
    pub functions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StructSpec {
    pub name: String,
    /// e.g. ["copy", "drop", "store", "key"]
    #[serde(default)]
    pub abilities: Vec<String>,
    #[serde(default)]
    pub fields: Vec<FieldSpec>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldSpec {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FunctionSpec {
    pub name: String,
    #[serde(default = "default_public")]
    pub visibility: String,
    #[serde(default)]
    pub is_entry: bool,
    #[serde(default)]
    pub parameters: Vec<String>,
    #[serde(default)]
    pub returns: Vec<String>,
    /// Additional locals beyond parameters.
    #[serde(default)]
    pub locals: Vec<String>,
    /// Bytecode instructions as strings, e.g. ["LdU64(42)", "Add", "Ret"].
    #[serde(default)]
    pub code: Vec<String>,
}

fn default_public() -> String {
    "public".to_string()
}

// ─── Build context ────────────────────────────────────────────────────────────

/// Internal lookup tables built incrementally as we process the spec.
pub struct BuildCtx {
    /// "normalized_address::module" → ModuleHandleIndex
    module_handle_map: HashMap<String, u16>,
    /// "normalized_address::module::Type" or "Self::Name" or "Name" → DatatypeHandleIndex
    datatype_handle_map: HashMap<String, u16>,
    /// "module::func" or "func" (for local fns) → FunctionHandleIndex
    function_handle_map: HashMap<String, u16>,
    /// Local struct name → StructDefinitionIndex
    struct_def_map: HashMap<String, u16>,
    /// Deduplicating signature pool; index 0 is always the empty signature.
    pub sig_pool: Vec<Signature>,
}

impl Default for BuildCtx {
    fn default() -> Self {
        Self::new()
    }
}

impl BuildCtx {
    pub fn new() -> Self {
        Self {
            module_handle_map: HashMap::new(),
            datatype_handle_map: HashMap::new(),
            function_handle_map: HashMap::new(),
            struct_def_map: HashMap::new(),
            // Index 0 must always be the empty signature (empty_module() requires it).
            sig_pool: vec![Signature(vec![])],
        }
    }

    pub fn intern_sig(&mut self, sig: Signature) -> SignatureIndex {
        for (i, s) in self.sig_pool.iter().enumerate() {
            if s.0 == sig.0 {
                return SignatureIndex(i as u16);
            }
        }
        let idx = self.sig_pool.len() as u16;
        self.sig_pool.push(sig);
        SignatureIndex(idx)
    }
}

// ─── Builder ──────────────────────────────────────────────────────────────────

/// Converts a `ModuleSpec` into a `CompiledModule`.
pub struct ModuleSpecBuilder;

impl ModuleSpecBuilder {
    pub fn build(spec: &ModuleSpec) -> Result<CompiledModule, String> {
        let mut module = empty_module();
        let mut ctx = BuildCtx::new();

        // empty_module() provides:
        //   identifiers[0]          = "DUMMY"
        //   address_identifiers[0]  = AccountAddress::ZERO
        //   module_handles[0]       = { address: 0, name: 0 }  ← self
        //   signatures[0]           = Signature([])
        //   self_module_handle_idx  = ModuleHandleIndex(0)

        // ── 1. Module name ──────────────────────────────────────────────────
        module.identifiers[0] = Identifier::new(spec.module_name.as_str())
            .map_err(|e| format!("invalid module name {:?}: {e}", spec.module_name))?;

        // ── 2. Imports ──────────────────────────────────────────────────────
        for imp in &spec.imports {
            let addr = parse_address(&imp.address)?;
            let norm_addr = normalize_addr(&addr);

            // Intern address in address_identifiers table.
            let addr_idx = if let Some(pos) =
                module.address_identifiers.iter().position(|a| *a == addr)
            {
                pos as u16
            } else {
                let idx = module.address_identifiers.len() as u16;
                module.address_identifiers.push(addr);
                idx
            };

            // Intern the module name identifier.
            let mod_id = Identifier::new(imp.module.as_str())
                .map_err(|e| format!("invalid import module {:?}: {e}", imp.module))?;
            let mod_ident_idx = intern_ident(&mut module.identifiers, mod_id);

            // Create module handle if not already present.
            let mh_key = format!("{}::{}", norm_addr, imp.module);
            let mh_idx = if let Some(&idx) = ctx.module_handle_map.get(&mh_key) {
                idx
            } else {
                let idx = module.module_handles.len() as u16;
                module.module_handles.push(ModuleHandle {
                    address: AddressIdentifierIndex(addr_idx),
                    name: IdentifierIndex(mod_ident_idx),
                });
                ctx.module_handle_map.insert(mh_key.clone(), idx);
                idx
            };

            // Create datatype handles for imported types.
            for type_name in &imp.types {
                let qualified = format!("{}::{}", mh_key, type_name);
                if ctx.datatype_handle_map.contains_key(&qualified) {
                    continue;
                }
                let type_id = Identifier::new(type_name.as_str())
                    .map_err(|e| format!("invalid type name {:?}: {e}", type_name))?;
                let type_ident_idx = intern_ident(&mut module.identifiers, type_id);
                let dt_idx = module.datatype_handles.len() as u16;
                module.datatype_handles.push(DatatypeHandle {
                    module: ModuleHandleIndex(mh_idx),
                    name: IdentifierIndex(type_ident_idx),
                    // Abilities unknown for imported types; BoundsChecker doesn't verify these.
                    abilities: AbilitySet::EMPTY,
                    type_parameters: vec![],
                });
                ctx.datatype_handle_map.insert(qualified, dt_idx);
                // Also register under shorthand "ModuleName::TypeName"
                let short = format!("{}::{}", imp.module, type_name);
                ctx.datatype_handle_map.entry(short).or_insert(dt_idx);
                // And plain type name (last-write wins for ambiguous names)
                ctx.datatype_handle_map
                    .entry(type_name.clone())
                    .or_insert(dt_idx);
            }

            // Create function handles for imported functions.
            for fn_name in &imp.functions {
                let short_key = format!("{}::{}", imp.module, fn_name);
                if ctx.function_handle_map.contains_key(&short_key) {
                    continue;
                }
                let fn_id = Identifier::new(fn_name.as_str())
                    .map_err(|e| format!("invalid function name {:?}: {e}", fn_name))?;
                let fn_ident_idx = intern_ident(&mut module.identifiers, fn_id);
                let fh_idx = module.function_handles.len() as u16;
                let empty_sig = ctx.intern_sig(Signature(vec![]));
                module.function_handles.push(FunctionHandle {
                    module: ModuleHandleIndex(mh_idx),
                    name: IdentifierIndex(fn_ident_idx),
                    parameters: empty_sig,
                    return_: empty_sig,
                    type_parameters: vec![],
                });
                ctx.function_handle_map.insert(short_key, fh_idx);
                // Register under fully qualified and bare name too.
                let full_key = format!("{}::{}", mh_key, fn_name);
                ctx.function_handle_map.insert(full_key, fh_idx);
                ctx.function_handle_map
                    .entry(fn_name.clone())
                    .or_insert(fh_idx);
            }
        }

        // ── 3. Register local struct names in handle/def maps ───────────────
        // We need two passes: first register DatatypeHandles so type resolution
        // works when parsing field types in the second pass.
        for (i, st) in spec.structs.iter().enumerate() {
            let abilities = parse_abilities(&st.abilities)?;
            let st_id = Identifier::new(st.name.as_str())
                .map_err(|e| format!("invalid struct name {:?}: {e}", st.name))?;
            let name_idx = intern_ident(&mut module.identifiers, st_id);
            let dt_idx = module.datatype_handles.len() as u16;
            module.datatype_handles.push(DatatypeHandle {
                module: ModuleHandleIndex(0), // defined in this module
                name: IdentifierIndex(name_idx),
                abilities,
                type_parameters: vec![],
            });
            let self_key = format!("Self::{}", st.name);
            ctx.datatype_handle_map.insert(self_key, dt_idx);
            ctx.datatype_handle_map.insert(st.name.clone(), dt_idx);
            ctx.struct_def_map.insert(st.name.clone(), i as u16);
        }

        // ── 4. Build struct definitions with field types ─────────────────────
        for st in &spec.structs {
            let dt_handle_idx = *ctx.struct_def_map.get(&st.name).unwrap();

            let mut fields = Vec::new();
            for field in &st.fields {
                let field_id = Identifier::new(field.name.as_str())
                    .map_err(|e| format!("invalid field name {:?}: {e}", field.name))?;
                let field_ident_idx = intern_ident(&mut module.identifiers, field_id);
                let sig_tok = parse_type(&field.type_, &ctx)?;
                fields.push(FieldDefinition {
                    name: IdentifierIndex(field_ident_idx),
                    signature: TypeSignature(sig_tok),
                });
            }

            // Move rejects zero-sized structs.
            if fields.is_empty() {
                let dummy_idx = ensure_ident(&mut module.identifiers, "field0");
                fields.push(FieldDefinition {
                    name: IdentifierIndex(dummy_idx),
                    signature: TypeSignature(SignatureToken::U64),
                });
            }

            let struct_def_idx = module.struct_defs.len() as u16;
            for (fi, _) in fields.iter().enumerate() {
                module.field_handles.push(FieldHandle {
                    owner: StructDefinitionIndex(struct_def_idx),
                    field: fi as u16,
                });
            }

            module.struct_defs.push(StructDefinition {
                struct_handle: DatatypeHandleIndex(dt_handle_idx),
                field_information: StructFieldInformation::Declared(fields),
            });
        }

        // ── 5. Create function handles for local functions ───────────────────
        for func in &spec.functions {
            let fn_id = Identifier::new(func.name.as_str())
                .map_err(|e| format!("invalid function name {:?}: {e}", func.name))?;
            let fn_ident_idx = intern_ident(&mut module.identifiers, fn_id);

            let mut param_toks = Vec::new();
            for p in &func.parameters {
                param_toks.push(parse_type(p, &ctx)?);
            }
            let mut ret_toks = Vec::new();
            for r in &func.returns {
                ret_toks.push(parse_type(r, &ctx)?);
            }

            let params_sig = ctx.intern_sig(Signature(param_toks));
            let ret_sig = ctx.intern_sig(Signature(ret_toks));
            let fh_idx = module.function_handles.len() as u16;
            module.function_handles.push(FunctionHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(fn_ident_idx),
                parameters: params_sig,
                return_: ret_sig,
                type_parameters: vec![],
            });
            ctx.function_handle_map.insert(func.name.clone(), fh_idx);
        }

        // ── 6. Build function definitions with bytecode ──────────────────────
        for func in &spec.functions {
            let fh_idx = *ctx.function_handle_map.get(&func.name).unwrap();

            let mut local_toks = Vec::new();
            for l in &func.locals {
                local_toks.push(parse_type(l, &ctx)?);
            }
            let locals_sig = ctx.intern_sig(Signature(local_toks));
            let visibility = parse_visibility(&func.visibility)?;

            let mut code: Vec<Bytecode> = Vec::new();
            for instr in &func.code {
                let bc = parse_bytecode(instr.trim(), &ctx)?;
                code.push(bc);
            }
            // Guarantee every function ends with Ret.
            if code.last() != Some(&Bytecode::Ret) {
                code.push(Bytecode::Ret);
            }

            module.function_defs.push(FunctionDefinition {
                function: FunctionHandleIndex(fh_idx),
                visibility,
                is_entry: func.is_entry,
                acquires_global_resources: vec![],
                code: Some(CodeUnit {
                    locals: locals_sig,
                    code,
                    jump_tables: vec![],
                }),
            });
        }

        // ── 7. Finalize: replace signature pool ──────────────────────────────
        module.signatures = ctx.sig_pool;

        Ok(module)
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn intern_ident(identifiers: &mut Vec<Identifier>, id: Identifier) -> u16 {
    if let Some(pos) = identifiers.iter().position(|i| *i == id) {
        return pos as u16;
    }
    let idx = identifiers.len() as u16;
    identifiers.push(id);
    idx
}

fn ensure_ident(identifiers: &mut Vec<Identifier>, name: &str) -> u16 {
    intern_ident(
        identifiers,
        Identifier::new(name).expect("hardcoded identifier is always valid"),
    )
}

/// Parse "0x2" or "0x000...002" into `AccountAddress`.
fn parse_address(s: &str) -> Result<AccountAddress, String> {
    let hex = s.trim().strip_prefix("0x").unwrap_or(s.trim());
    if hex.len() > 64 {
        return Err(format!("address too long: {s:?}"));
    }
    let padded = format!("{:0>64}", hex);
    let bytes: Vec<u8> = (0..32)
        .map(|i| {
            u8::from_str_radix(&padded[i * 2..i * 2 + 2], 16)
                .map_err(|e| format!("bad hex in address {s:?}: {e}"))
        })
        .collect::<Result<_, _>>()?;
    Ok(AccountAddress::new(bytes.try_into().unwrap()))
}

/// Canonical address string for map keys: 64-char lowercase hex without "0x".
fn normalize_addr(addr: &AccountAddress) -> String {
    let bytes = addr.to_vec();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn parse_abilities(abilities: &[String]) -> Result<AbilitySet, String> {
    let mut set = AbilitySet::EMPTY;
    for a in abilities {
        let ability = match a.to_lowercase().as_str() {
            "copy" => Ability::Copy,
            "drop" => Ability::Drop,
            "store" => Ability::Store,
            "key" => Ability::Key,
            other => return Err(format!("unknown ability {other:?}")),
        };
        set = set | ability;
    }
    Ok(set)
}

fn parse_visibility(s: &str) -> Result<Visibility, String> {
    match s.to_lowercase().as_str() {
        "public" => Ok(Visibility::Public),
        "private" | "" => Ok(Visibility::Private),
        "friend" | "package" => Ok(Visibility::Friend),
        other => Err(format!("unknown visibility {other:?}")),
    }
}

/// Parse a type string into a `SignatureToken`.
///
/// Supported forms:
/// - Primitives: `bool`, `u8`, `u16`, `u32`, `u64`, `u128`, `u256`, `address`, `signer`
/// - References: `&T`, `&mut T`
/// - Vectors: `vector<T>`
/// - Qualified names: `0x2::object::UID`, `Self::MyStruct`, or plain `MyStruct`
/// - Shorthands: `TxContext` → `0x2::tx_context::TxContext`, `UID` → `0x2::object::UID`
/// - Type parameters: single uppercase letters like `T`, `E` → `TypeParameter(0)`
pub fn parse_type(s: &str, ctx: &BuildCtx) -> Result<SignatureToken, String> {
    let s = s.trim();

    // Shorthands for common Sui framework types.
    match s {
        "TxContext" => {
            // Resolve as 0x2::tx_context::TxContext if imported, else fall through.
            let key = format!(
                "{}::tx_context::TxContext",
                normalize_addr(&AccountAddress::new([
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 2
                ]))
            );
            if let Some(&idx) = ctx.datatype_handle_map.get(&key) {
                return Ok(SignatureToken::Datatype(DatatypeHandleIndex(idx)));
            }
            if let Some(&idx) = ctx.datatype_handle_map.get("TxContext") {
                return Ok(SignatureToken::Datatype(DatatypeHandleIndex(idx)));
            }
            if let Some(&idx) = ctx.datatype_handle_map.get("tx_context::TxContext") {
                return Ok(SignatureToken::Datatype(DatatypeHandleIndex(idx)));
            }
        }
        "UID" => {
            if let Some(&idx) = ctx.datatype_handle_map.get("UID") {
                return Ok(SignatureToken::Datatype(DatatypeHandleIndex(idx)));
            }
        }
        "&mut TxContext" => {
            return Ok(SignatureToken::MutableReference(Box::new(parse_type(
                "TxContext",
                ctx,
            )?)));
        }
        "&TxContext" => {
            return Ok(SignatureToken::Reference(Box::new(parse_type(
                "TxContext",
                ctx,
            )?)));
        }
        _ => {}
    }

    match s {
        "bool" => return Ok(SignatureToken::Bool),
        "u8" => return Ok(SignatureToken::U8),
        "u16" => return Ok(SignatureToken::U16),
        "u32" => return Ok(SignatureToken::U32),
        "u64" => return Ok(SignatureToken::U64),
        "u128" => return Ok(SignatureToken::U128),
        "u256" => return Ok(SignatureToken::U256),
        "address" => return Ok(SignatureToken::Address),
        "signer" => return Ok(SignatureToken::Signer),
        _ => {}
    }

    if let Some(inner) = s.strip_prefix("&mut ") {
        return Ok(SignatureToken::MutableReference(Box::new(parse_type(
            inner, ctx,
        )?)));
    }
    if let Some(inner) = s.strip_prefix('&') {
        return Ok(SignatureToken::Reference(Box::new(parse_type(inner, ctx)?)));
    }
    if let Some(inner) = s
        .strip_prefix("vector<")
        .and_then(|t| t.strip_suffix('>'))
    {
        return Ok(SignatureToken::Vector(Box::new(parse_type(inner, ctx)?)));
    }

    // Try datatype resolution (handles qualified names and local names).
    if let Some(&idx) = ctx.datatype_handle_map.get(s) {
        return Ok(SignatureToken::Datatype(DatatypeHandleIndex(idx)));
    }

    // For "0x2::module::Type", try normalizing the address component.
    if let Some(normalized) = try_normalize_qualified(s)
        && let Some(&idx) = ctx.datatype_handle_map.get(&normalized)
    {
        return Ok(SignatureToken::Datatype(DatatypeHandleIndex(idx)));
    }

    // Single uppercase letter(s) → type parameter.
    if !s.is_empty() && s.len() <= 2 && s.chars().all(|c| c.is_ascii_uppercase()) {
        return Ok(SignatureToken::TypeParameter(0));
    }

    Err(format!(
        "unknown type {s:?}; did you forget to list it in the imports section?"
    ))
}

/// Attempt to normalize a qualified type string ("0x2::mod::Type") so the
/// address component matches what's stored in `datatype_handle_map`.
fn try_normalize_qualified(s: &str) -> Option<String> {
    // Expect at least two "::" separators.
    let mut parts = s.splitn(3, "::");
    let addr_part = parts.next()?;
    let mod_part = parts.next()?;
    let type_part = parts.next()?;
    let addr = parse_address(addr_part).ok()?;
    Some(format!(
        "{}::{}::{}",
        normalize_addr(&addr),
        mod_part,
        type_part
    ))
}

/// Parse a bytecode instruction string into a `Bytecode` variant.
///
/// Formats:
/// - No-arg:        `"Ret"`, `"Add"`, `"Pop"`, `"LdTrue"`, etc.
/// - Integer arg:   `"LdU64(42)"`, `"Branch(5)"`, `"CopyLoc(0)"`, etc.
/// - Named ref:     `"Pack(MyStruct)"`, `"Unpack(MyStruct)"`, `"Call(object::new)"`, etc.
pub fn parse_bytecode(s: &str, ctx: &BuildCtx) -> Result<Bytecode, String> {
    let (name, arg) = if let Some(pos) = s.find('(') {
        if !s.ends_with(')') {
            return Err(format!("malformed instruction {s:?}: missing ')'"));
        }
        (&s[..pos], Some(s[pos + 1..s.len() - 1].trim()))
    } else {
        (s, None)
    };
    let name = name.trim();

    macro_rules! no_arg {
        ($bc:expr) => {{
            if arg.is_some() {
                return Err(format!("{name} takes no argument"));
            }
            Ok($bc)
        }};
    }

    macro_rules! u8_arg {
        ($bc:ident) => {{
            let a = arg.ok_or_else(|| format!("{name} requires an argument"))?;
            let n: u8 = a
                .parse()
                .map_err(|_| format!("{name} argument must be u8, got {a:?}"))?;
            Ok(Bytecode::$bc(n))
        }};
    }

    macro_rules! u16_arg {
        ($bc:ident) => {{
            let a = arg.ok_or_else(|| format!("{name} requires an argument"))?;
            let n: u16 = a
                .parse()
                .map_err(|_| format!("{name} argument must be u16, got {a:?}"))?;
            Ok(Bytecode::$bc(n))
        }};
    }

    match name {
        // ── No-argument instructions ────────────────────────────────────────
        "Ret" => no_arg!(Bytecode::Ret),
        "Pop" => no_arg!(Bytecode::Pop),
        "LdTrue" => no_arg!(Bytecode::LdTrue),
        "LdFalse" => no_arg!(Bytecode::LdFalse),
        "Abort" => no_arg!(Bytecode::Abort),
        "Nop" => no_arg!(Bytecode::Nop),
        "FreezeRef" => no_arg!(Bytecode::FreezeRef),
        "ReadRef" => no_arg!(Bytecode::ReadRef),
        "WriteRef" => no_arg!(Bytecode::WriteRef),
        "Add" => no_arg!(Bytecode::Add),
        "Sub" => no_arg!(Bytecode::Sub),
        "Mul" => no_arg!(Bytecode::Mul),
        "Div" => no_arg!(Bytecode::Div),
        "Mod" => no_arg!(Bytecode::Mod),
        "BitAnd" => no_arg!(Bytecode::BitAnd),
        "BitOr" => no_arg!(Bytecode::BitOr),
        "Xor" => no_arg!(Bytecode::Xor),
        "Shl" => no_arg!(Bytecode::Shl),
        "Shr" => no_arg!(Bytecode::Shr),
        "Or" => no_arg!(Bytecode::Or),
        "And" => no_arg!(Bytecode::And),
        "Not" => no_arg!(Bytecode::Not),
        "Lt" => no_arg!(Bytecode::Lt),
        "Gt" => no_arg!(Bytecode::Gt),
        "Le" => no_arg!(Bytecode::Le),
        "Ge" => no_arg!(Bytecode::Ge),
        "Eq" => no_arg!(Bytecode::Eq),
        "Neq" => no_arg!(Bytecode::Neq),
        "CastU8" => no_arg!(Bytecode::CastU8),
        "CastU16" => no_arg!(Bytecode::CastU16),
        "CastU32" => no_arg!(Bytecode::CastU32),
        "CastU64" => no_arg!(Bytecode::CastU64),
        "CastU128" => no_arg!(Bytecode::CastU128),
        "CastU256" => no_arg!(Bytecode::CastU256),

        // ── Integer constant instructions ───────────────────────────────────
        "LdU8" => {
            let a = arg.ok_or_else(|| "LdU8 requires an argument".to_string())?;
            let n: u8 = a
                .parse()
                .map_err(|_| format!("LdU8 argument must be u8, got {a:?}"))?;
            Ok(Bytecode::LdU8(n))
        }
        "LdU16" => {
            let a = arg.ok_or_else(|| "LdU16 requires an argument".to_string())?;
            let n: u16 = a
                .parse()
                .map_err(|_| format!("LdU16 argument must be u16, got {a:?}"))?;
            Ok(Bytecode::LdU16(n))
        }
        "LdU32" => {
            let a = arg.ok_or_else(|| "LdU32 requires an argument".to_string())?;
            let n: u32 = a
                .parse()
                .map_err(|_| format!("LdU32 argument must be u32, got {a:?}"))?;
            Ok(Bytecode::LdU32(n))
        }
        "LdU64" => {
            let a = arg.ok_or_else(|| "LdU64 requires an argument".to_string())?;
            let n: u64 = a
                .parse()
                .map_err(|_| format!("LdU64 argument must be u64, got {a:?}"))?;
            Ok(Bytecode::LdU64(n))
        }
        "LdU128" => {
            let a = arg.ok_or_else(|| "LdU128 requires an argument".to_string())?;
            let n: u128 = a
                .parse()
                .map_err(|_| format!("LdU128 argument must be u128, got {a:?}"))?;
            Ok(Bytecode::LdU128(Box::new(n)))
        }
        "LdU256" => {
            let a = arg.ok_or_else(|| "LdU256 requires an argument".to_string())?;
            // Accept as u64-sized decimal for convenience.
            let n: u64 = a
                .parse()
                .map_err(|_| format!("LdU256 argument must be a decimal integer, got {a:?}"))?;
            Ok(Bytecode::LdU256(Box::new(
                move_core_types::u256::U256::from(n),
            )))
        }

        // ── Local variable instructions ─────────────────────────────────────
        "CopyLoc" => u8_arg!(CopyLoc),
        "MoveLoc" => u8_arg!(MoveLoc),
        "StLoc" => u8_arg!(StLoc),
        "ImmBorrowLoc" => u8_arg!(ImmBorrowLoc),
        "MutBorrowLoc" => u8_arg!(MutBorrowLoc),

        // ── Branch instructions ─────────────────────────────────────────────
        "Branch" => u16_arg!(Branch),
        "BrTrue" => u16_arg!(BrTrue),
        "BrFalse" => u16_arg!(BrFalse),

        // ── Struct pack/unpack ──────────────────────────────────────────────
        "Pack" | "Unpack" => {
            let a = arg.ok_or_else(|| format!("{name} requires a struct name or index"))?;
            let sdi = resolve_struct(a, ctx)?;
            Ok(match name {
                "Pack" => Bytecode::Pack(sdi),
                _ => Bytecode::Unpack(sdi),
            })
        }

        // ── Field borrow ────────────────────────────────────────────────────
        "ImmBorrowField" => {
            let a = arg.ok_or_else(|| "ImmBorrowField requires a field handle index".to_string())?;
            let n: u16 = a
                .parse()
                .map_err(|_| format!("ImmBorrowField argument must be u16, got {a:?}"))?;
            Ok(Bytecode::ImmBorrowField(FieldHandleIndex(n)))
        }
        "MutBorrowField" => {
            let a = arg.ok_or_else(|| "MutBorrowField requires a field handle index".to_string())?;
            let n: u16 = a
                .parse()
                .map_err(|_| format!("MutBorrowField argument must be u16, got {a:?}"))?;
            Ok(Bytecode::MutBorrowField(FieldHandleIndex(n)))
        }

        // ── Function calls ──────────────────────────────────────────────────
        "Call" => {
            let a = arg.ok_or_else(|| "Call requires a function reference".to_string())?;
            let fh_idx = ctx
                .function_handle_map
                .get(a)
                .ok_or_else(|| format!("Call: unknown function {a:?}; import it first"))?;
            Ok(Bytecode::Call(FunctionHandleIndex(*fh_idx)))
        }

        // ── Vector operations ───────────────────────────────────────────────
        "VecLen" => {
            let a = arg.ok_or_else(|| "VecLen requires a signature index".to_string())?;
            let n: u16 = a
                .parse()
                .map_err(|_| format!("VecLen argument must be u16, got {a:?}"))?;
            Ok(Bytecode::VecLen(SignatureIndex(n)))
        }
        "VecImmBorrow" => {
            let a = arg.ok_or_else(|| "VecImmBorrow requires a signature index".to_string())?;
            let n: u16 = a
                .parse()
                .map_err(|_| format!("VecImmBorrow argument must be u16, got {a:?}"))?;
            Ok(Bytecode::VecImmBorrow(SignatureIndex(n)))
        }
        "VecMutBorrow" => {
            let a = arg.ok_or_else(|| "VecMutBorrow requires a signature index".to_string())?;
            let n: u16 = a
                .parse()
                .map_err(|_| format!("VecMutBorrow argument must be u16, got {a:?}"))?;
            Ok(Bytecode::VecMutBorrow(SignatureIndex(n)))
        }
        "VecPushBack" => {
            let a = arg.ok_or_else(|| "VecPushBack requires a signature index".to_string())?;
            let n: u16 = a
                .parse()
                .map_err(|_| format!("VecPushBack argument must be u16, got {a:?}"))?;
            Ok(Bytecode::VecPushBack(SignatureIndex(n)))
        }
        "VecPopBack" => {
            let a = arg.ok_or_else(|| "VecPopBack requires a signature index".to_string())?;
            let n: u16 = a
                .parse()
                .map_err(|_| format!("VecPopBack argument must be u16, got {a:?}"))?;
            Ok(Bytecode::VecPopBack(SignatureIndex(n)))
        }
        "VecSwap" => {
            let a = arg.ok_or_else(|| "VecSwap requires a signature index".to_string())?;
            let n: u16 = a
                .parse()
                .map_err(|_| format!("VecSwap argument must be u16, got {a:?}"))?;
            Ok(Bytecode::VecSwap(SignatureIndex(n)))
        }

        other => Err(format!(
            "unknown instruction {other:?}; \
             check spelling or use a supported opcode"
        )),
    }
}

/// Resolve a struct argument (name or numeric index) to a `StructDefinitionIndex`.
fn resolve_struct(a: &str, ctx: &BuildCtx) -> Result<StructDefinitionIndex, String> {
    if let Ok(n) = a.parse::<u16>() {
        return Ok(StructDefinitionIndex(n));
    }
    ctx.struct_def_map
        .get(a)
        .map(|&idx| StructDefinitionIndex(idx))
        .ok_or_else(|| format!("unknown struct {a:?}; define it in the structs section"))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sui_harness;

    fn build(json: &str) -> Result<CompiledModule, String> {
        let spec: ModuleSpec = serde_json::from_str(json).map_err(|e| e.to_string())?;
        ModuleSpecBuilder::build(&spec)
    }

    #[test]
    fn simple_module_roundtrips() {
        let module = build(
            r#"{
            "module_name": "test_mod",
            "structs": [{
                "name": "MyVal",
                "abilities": ["copy", "drop"],
                "fields": [
                    {"name": "x", "type": "u64"},
                    {"name": "y", "type": "bool"}
                ]
            }],
            "functions": [{
                "name": "do_thing",
                "visibility": "public",
                "parameters": ["u64"],
                "returns": ["u64"],
                "locals": ["u64"],
                "code": ["CopyLoc(0)", "LdU64(1)", "Add", "Ret"]
            }]
        }"#,
        )
        .expect("build");

        assert_eq!(module.struct_defs.len(), 1);
        assert_eq!(module.function_defs.len(), 1);
        sui_harness::roundtrip_check(&module).expect("roundtrip");
    }

    #[test]
    fn module_passes_bounds_check() {
        let module = build(
            r#"{
            "module_name": "bounds_mod",
            "functions": [{
                "name": "f",
                "parameters": [],
                "returns": [],
                "locals": [],
                "code": ["LdU64(0)", "Pop", "Ret"]
            }]
        }"#,
        )
        .expect("build");

        let mut bytes = Vec::new();
        module.serialize(&mut bytes).expect("serialize");
        let config = move_binary_format::binary_config::BinaryConfig::standard();
        let result =
            move_binary_format::file_format::CompiledModule::deserialize_with_config(&bytes, &config);
        assert!(result.is_ok(), "bounds check failed: {result:?}");
    }

    #[test]
    fn module_with_import_roundtrips() {
        let module = build(
            r#"{
            "module_name": "fuzz_obj",
            "imports": [{"address": "0x2", "module": "object", "types": ["UID"], "functions": []}],
            "structs": [{
                "name": "MyObj",
                "abilities": ["key", "store"],
                "fields": [
                    {"name": "id", "type": "UID"},
                    {"name": "val", "type": "u64"}
                ]
            }],
            "functions": [{
                "name": "empty",
                "parameters": [],
                "returns": [],
                "locals": [],
                "code": ["Ret"]
            }]
        }"#,
        )
        .expect("build");

        sui_harness::roundtrip_check(&module).expect("roundtrip");
    }

    #[test]
    fn type_parsing_primitives() {
        let ctx = BuildCtx::new();
        assert_eq!(parse_type("bool", &ctx).unwrap(), SignatureToken::Bool);
        assert_eq!(parse_type("u64", &ctx).unwrap(), SignatureToken::U64);
        assert_eq!(parse_type("u8", &ctx).unwrap(), SignatureToken::U8);
        assert_eq!(parse_type("u128", &ctx).unwrap(), SignatureToken::U128);
        assert_eq!(parse_type("address", &ctx).unwrap(), SignatureToken::Address);
    }

    #[test]
    fn type_parsing_references_and_vectors() {
        let ctx = BuildCtx::new();
        assert_eq!(
            parse_type("&u64", &ctx).unwrap(),
            SignatureToken::Reference(Box::new(SignatureToken::U64))
        );
        assert_eq!(
            parse_type("&mut u64", &ctx).unwrap(),
            SignatureToken::MutableReference(Box::new(SignatureToken::U64))
        );
        assert_eq!(
            parse_type("vector<u8>", &ctx).unwrap(),
            SignatureToken::Vector(Box::new(SignatureToken::U8))
        );
        assert_eq!(
            parse_type("vector<&u64>", &ctx).unwrap(),
            SignatureToken::Vector(Box::new(SignatureToken::Reference(Box::new(
                SignatureToken::U64
            ))))
        );
    }

    #[test]
    fn bytecode_parsing_no_arg() {
        let ctx = BuildCtx::new();
        assert_eq!(parse_bytecode("Ret", &ctx).unwrap(), Bytecode::Ret);
        assert_eq!(parse_bytecode("Add", &ctx).unwrap(), Bytecode::Add);
        assert_eq!(parse_bytecode("Pop", &ctx).unwrap(), Bytecode::Pop);
        assert_eq!(parse_bytecode("LdTrue", &ctx).unwrap(), Bytecode::LdTrue);
        assert_eq!(parse_bytecode("FreezeRef", &ctx).unwrap(), Bytecode::FreezeRef);
    }

    #[test]
    fn bytecode_parsing_with_args() {
        let ctx = BuildCtx::new();
        assert_eq!(parse_bytecode("LdU64(42)", &ctx).unwrap(), Bytecode::LdU64(42));
        assert_eq!(parse_bytecode("LdU8(255)", &ctx).unwrap(), Bytecode::LdU8(255));
        assert_eq!(parse_bytecode("CopyLoc(2)", &ctx).unwrap(), Bytecode::CopyLoc(2));
        assert_eq!(parse_bytecode("Branch(10)", &ctx).unwrap(), Bytecode::Branch(10));
        assert_eq!(parse_bytecode("BrTrue(5)", &ctx).unwrap(), Bytecode::BrTrue(5));
        assert_eq!(parse_bytecode("BrFalse(0)", &ctx).unwrap(), Bytecode::BrFalse(0));
        assert_eq!(parse_bytecode("StLoc(1)", &ctx).unwrap(), Bytecode::StLoc(1));
    }

    #[test]
    fn bytecode_call_resolution() {
        let mut ctx = BuildCtx::new();
        ctx.function_handle_map.insert("object::new".to_string(), 1);
        assert_eq!(
            parse_bytecode("Call(object::new)", &ctx).unwrap(),
            Bytecode::Call(FunctionHandleIndex(1))
        );
    }

    #[test]
    fn bytecode_pack_by_name() {
        let mut ctx = BuildCtx::new();
        ctx.struct_def_map.insert("MyStruct".to_string(), 0);
        assert_eq!(
            parse_bytecode("Pack(MyStruct)", &ctx).unwrap(),
            Bytecode::Pack(StructDefinitionIndex(0))
        );
        assert_eq!(
            parse_bytecode("Unpack(MyStruct)", &ctx).unwrap(),
            Bytecode::Unpack(StructDefinitionIndex(0))
        );
    }

    #[test]
    fn multi_function_module() {
        let module = build(
            r#"{
            "module_name": "multi",
            "functions": [
                {
                    "name": "helper",
                    "visibility": "private",
                    "parameters": ["u64"],
                    "returns": ["u64"],
                    "locals": [],
                    "code": ["CopyLoc(0)", "LdU64(1)", "Add", "Ret"]
                },
                {
                    "name": "entry_fn",
                    "visibility": "public",
                    "is_entry": true,
                    "parameters": ["u64"],
                    "returns": [],
                    "locals": [],
                    "code": ["MoveLoc(0)", "Pop", "Ret"]
                }
            ]
        }"#,
        )
        .expect("build");

        assert_eq!(module.function_defs.len(), 2);
        sui_harness::roundtrip_check(&module).expect("roundtrip");
    }

    #[test]
    fn branching_module() {
        // Tests BrFalse / Branch offset calculation.
        let module = build(
            r#"{
            "module_name": "branch_mod",
            "functions": [{
                "name": "choose",
                "parameters": ["bool"],
                "returns": ["u64"],
                "locals": [],
                "code": [
                    "CopyLoc(0)",
                    "BrFalse(4)",
                    "LdU64(1)",
                    "Branch(5)",
                    "LdU64(0)",
                    "Ret"
                ]
            }]
        }"#,
        )
        .expect("build");

        let mut bytes = Vec::new();
        module.serialize(&mut bytes).expect("serialize");
        let config = move_binary_format::binary_config::BinaryConfig::standard();
        let result =
            move_binary_format::file_format::CompiledModule::deserialize_with_config(&bytes, &config);
        assert!(result.is_ok(), "bounds check: {result:?}");
    }
}
