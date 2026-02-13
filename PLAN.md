# Plan: Move Package Diff Tool

## Goal
Build a library crate that detects changes between two versions of a Move bytecode package.
Input: two `&MovePackage` values (before/after).
Output: a structured diff showing types added/removed, functions added/removed, and functions changed (with decompiled textual diffs).

---

## New Crate: `crates/sui-move-package-diff/`

### Output Data Structure

```rust
pub struct PackageDiff {
    pub module_diffs: BTreeMap<String, ModuleDiff>,
    pub modules_added: Vec<String>,
    pub modules_removed: Vec<String>,
}

pub struct ModuleDiff {
    pub types_added: Vec<String>,
    pub types_removed: Vec<String>,
    pub functions_added: Vec<String>,
    pub functions_removed: Vec<String>,
    pub functions_changed: Vec<FunctionChanged>,
}

pub struct FunctionChanged {
    pub name: String,
    pub before_text: String,  // decompiled source of old version
    pub after_text: String,   // decompiled source of new version
}
```

### No changes to existing crates

### Approach

#### Phase 1: Structural diff via `normalized::Module`

1. Call `MovePackage::normalize()` on both packages with `include_code: true` to get `BTreeMap<String, normalized::Module<S>>` for each.
2. Compare module name sets → `modules_added`, `modules_removed`.
3. For modules present in both:
   - Compare struct+enum name sets → `types_added`, `types_removed`
   - Compare function name sets → `functions_added`, `functions_removed`
   - For functions present in both, use `Function::equivalent()` to detect changes

#### Phase 2: Decompiled diff for changed functions

For each package (before and after) that has changed functions:

1. Deserialize all bytecode modules from `MovePackage::serialized_module_map()` into `Vec<CompiledModule>`
2. Build `Model<WithoutSource>` via `compiled_model::Model::from_compiled_with_config()` with `allow_missing_dependencies: true`
3. Run the decompiler: `move_decompiler::translate::model(model)` → `Decompiled<WithoutSource>`
4. For each changed function, build a synthetic single-function `ast::Module`:
   - Clone the `ast::Module` from the decompiled output
   - Replace its `functions` map with a single entry containing only the changed function
5. Render via `pretty_printer::module(model, pkg_name, model_mod, &single_fn_module)` → `Doc` → `String`
6. Extract the function text by finding the `// -- functions --` marker and taking everything after it
7. Store before/after text in `FunctionChanged`

**Why this works**: `pretty_printer::module()` iterates only over `ast::Module.functions` when rendering the functions section. By providing a synthetic module with one function, only that function is rendered. The `model_mod` (from the Model) still provides the correct function signature via `model_mod.maybe_function(name)`.

### File structure

```
crates/sui-move-package-diff/
├── Cargo.toml
├── src/
│   └── lib.rs
```

### Dependencies
- `sui-types` (for `MovePackage`)
- `move-binary-format` (for `normalized`, `CompiledModule`, `BinaryConfig`)
- `move-decompiler` (for `translate::model`, `pretty_printer::module`, `ast`)
- `move-model-2` (for `Model<WithoutSource>`, `compiled_model`, `ModelConfig`)
- `move-symbol-pool` (for `Symbol`)
- `anyhow`

### Implementation steps

1. Create `crates/sui-move-package-diff/Cargo.toml`
2. Implement `lib.rs`:
   - `pub fn diff_packages(before: &MovePackage, after: &MovePackage) -> anyhow::Result<PackageDiff>`
   - Internal helper: normalize both packages, compare sets
   - Internal helper: decompile package, render single function
3. Register the crate in the workspace `Cargo.toml`
4. Run `cargo check -p sui-move-package-diff`
5. Run `cargo xclippy`
