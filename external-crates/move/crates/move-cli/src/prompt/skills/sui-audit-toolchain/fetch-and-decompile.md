# Fetch, disassemble (analyze), and decompile (explain)

Goal: produce the **analysis substrate** for the audit (the disassembly of every module) and the
matching **explanation layer** (decompiled `.move` of every module), from a target package.
Assumes `setup.md` is done: `$MOVE_BIN` points at the freshly-built `move` binary in
`$AUDIT_WORK`, and `sui` resolves to the `suiup`-managed CLI (provenance-checked in `setup.md`
step 1). Every `sui` invocation below uses that same provenance-checked binary — no other.

> Roles, restated: **`.asm` is the analysis source of truth (1:1 with executed bytecode).**
> **`.move` (decompiled) is a heuristic reconstruction used only to render confirmed findings
> for human readers.** Do not derive findings from `.move`.

## 1. Get the `.mv` modules

### Route A — fetch on-chain bytecode by package id

Make sure the CLI is pointed at the right network first (`sui client active-env`; switch with
`sui client switch --env mainnet` etc. — the package id is network-specific).

```sh
PKG=0x<package_id>
OUT="$AUDIT_WORK/target/$PKG"
mkdir -p "$OUT/mv"

# Fetch the package object as JSON. Package bytecode lives in:
#   .content.Package.module_map  ->  { "<module_name>": [<u8 bytes...>], ... }
#   (the byte array starts with the Move magic 161,28,235,11 = 0xA11CEB0B)
sui client object "$PKG" --json > "$OUT/package.json"

# Write each module's bytes to a .mv file (robust int-array -> binary).
python3 - "$OUT/package.json" "$OUT/mv" <<'PY'
import json, os, sys
data = json.load(open(sys.argv[1])); outdir = sys.argv[2]
os.makedirs(outdir, exist_ok=True)
mods = data["content"]["Package"]["module_map"]
for name, byts in mods.items():
    open(os.path.join(outdir, name + ".mv"), "wb").write(bytes(byts))
print(f"wrote {len(mods)} modules to {outdir}")
PY
```

If a different `sui` version emits base64 strings instead of int arrays under `module_map`, swap
the inner write for `base64.b64decode(byts)`. Verify a file is real bytecode:
`xxd "$OUT/mv/"*.mv | head -1` should begin with `a11c eb0b`.

### Route B — package supplied directly

If given `.mv` files or a build output dir, point at it directly — e.g. a compiled package's
`build/<pkg>/bytecode_modules/` already contains `.mv` files. Set `OUT/mv` to that directory (or
copy the files in). No fetch needed.

## 2. Disassemble every module — THIS is the analysis input

```sh
mkdir -p "$OUT/asm"
for mv in "$OUT/mv/"*.mv; do
  base=$(basename "$mv" .mv)
  sui move disassemble "$mv" > "$OUT/asm/$base.asm"
done
ls "$OUT/asm/"
```

Feed `$OUT/asm/*.asm` to `sui-move-security-review`. The auditor reasons over the assembly:
faithful opcodes, exact `Call` symbols (e.g. `Call transfer::transfer<T>` vs
`Call transfer::public_transfer<T>`), `CastU*` instructions, abort code values, ability sets on
struct headers, visibility/`entry` on function headers. **Do not switch surfaces** mid-analysis.

## 3. Decompile every module — for finding-explanation only

```sh
"$MOVE_BIN" decompile --input "$OUT/mv" --output "$OUT/decompiled"
# Recurses the input dir for *.mv; deserializes each module; tolerates missing dependencies
# (allow_missing_dependencies = true), so a single package decompiles without its deps.
# Output: $OUT/decompiled/<package>/<module>.move
ls -R "$OUT/decompiled"
```

The decompiled `.move` is reached for **only after a finding is confirmed on the assembly**, to
render the matching construct as readable Move alongside the finding ("Human view" in the report).
If the decompiled view disagrees with the assembly, the **assembly wins** — that disagreement is
itself information about decompiler imprecision, not about the package.

Read `move-bytecode-comprehension/reading-decompiled.md` for the decompiler's known artifacts (e.g.
constants renamed `C0/C1…`, empty structs gain `dummy_field: bool`, locals invented, macros
expanded) so you can present without misreading them.

## Hand-off

Report to the audit step: the **assembly dir** (`$OUT/asm/`) as the analysis substrate, the matched
**decompiled dir** (`$OUT/decompiled/`) for finding-explanation, the module list, the network +
package id (or file provenance), and `SUI_REF` — so the finding set is reproducible.
