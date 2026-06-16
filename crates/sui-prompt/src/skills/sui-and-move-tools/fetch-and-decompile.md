# Fetch bytecode + produce a readable view of a deployed Sui package

Produces `.mv` (raw bytecode) and `.move` (decompiled source) files for every module in
the package. The decompiled view is the working substrate for most reading and analysis;
disassembly is fetched per-module on demand for specific verification cases (see the
final section).

## 1. Choose network + target

```sh
PKG=0x<package_id>
NETWORK=mainnet                 # or testnet, devnet
GQL="https://graphql.${NETWORK}.sui.io/graphql"
OUT="./.move-work/$PKG"
mkdir -p "$OUT/mv"
```

## 2. One GraphQL call → bytes for every module

```sh
curl -s "$GQL" -H 'Content-Type: application/json' \
  -d "{\"query\":\"{ object(address:\\\"$PKG\\\") { asMovePackage { modules { nodes { name bytes } } } } }\"}" \
  > "$OUT/package.json"
jq '.errors // "ok"' "$OUT/package.json"   # sanity-check
```

The response, under `data.object.asMovePackage.modules.nodes[]`, has one entry per module
with `{name, bytes}`. `bytes` is Base64-encoded raw bytecode.

**Note:** the query deliberately does not request `disassembly`. Asking for it pulls the
text into `package.json`, where it leaks into the agent's context if the file is later
inspected. Fetch disassembly per-module on demand instead (see the final section).

## 3. Write per-module .mv files

```sh
jq -c '.data.object.asMovePackage.modules.nodes[]' "$OUT/package.json" | while read -r mod; do
  name=$(jq -r '.name' <<<"$mod")
  jq -r '.bytes'       <<<"$mod" | base64 -d > "$OUT/mv/$name.mv"
done

# Magic-byte sanity (each .mv should start with a11ceb0b)
for f in "$OUT/mv/"*.mv; do
  head -c 4 "$f" | xxd -p | grep -q '^a11ceb0b' || echo "WARN: $f missing magic"
done
```

## 4. Decompile for the working view

```sh
sui move decompile --input "$OUT/mv" --output "$OUT/decompiled"
```

The decompiled `.move` is a heuristic reconstruction; see
`move-bytecode-comprehension/decompilation.md` for what's faithful and what's an
artifact to recognize.

## Output paths

- `$OUT/mv/<module>.mv`                       — raw bytecode
- `$OUT/decompiled/<package>/<module>.move`   — decompiled source (the working view)

---

## Fetching disassembly on demand (one module at a time)

Only run this when a specific question can't be answered from the decompiled view —
e.g. looking up an abort code's numeric value, or when decompilation failed for the
module you care about. The query is per-module to avoid pulling unrelated modules'
disassembly text into anywhere the agent might later read.

```sh
MODULE=<module_name>            # e.g. pool, vault, registry
mkdir -p "$OUT/asm"
curl -s "$GQL" -H 'Content-Type: application/json' \
  -d "{\"query\":\"{ object(address:\\\"$PKG\\\") { asMovePackage { module(name:\\\"$MODULE\\\") { disassembly } } } }\"}" \
  | jq -r '.data.object.asMovePackage.module.disassembly' > "$OUT/asm/$MODULE.asm"
```

The `.asm` file lives alongside the `.mv` and `.move` files for that module. See
`move-bytecode-comprehension/disassembly.md` for how to read it.

**Read it surgically.** A whole `.asm` for a complex module can run tens of thousands
of tokens. When inspecting, use `grep` / `sed` for the specific function or basic block
rather than dumping the full file into context.
