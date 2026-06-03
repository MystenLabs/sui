# Fetch bytecode + produce readable views for a deployed Sui package

Produces `.mv` (raw bytecode), `.asm` (disassembly), and optionally `.move` (decompiled
source) files for every module in the package.

## 1. Choose network + target

```sh
PKG=0x<package_id>
NETWORK=mainnet                 # or testnet, devnet
GQL="https://graphql.${NETWORK}.sui.io/graphql"
OUT="./.move-work/$PKG"
mkdir -p "$OUT/mv" "$OUT/asm"
```

## 2. One GraphQL call → bytes + disassembly for every module

```sh
curl -s "$GQL" -H 'Content-Type: application/json' \
  -d "{\"query\":\"{ object(address:\\\"$PKG\\\") { asMovePackage { modules { nodes { name bytes disassembly } } } } }\"}" \
  > "$OUT/package.json"
jq '.errors // "ok"' "$OUT/package.json"   # sanity-check
```

The response, under `data.object.asMovePackage.modules.nodes[]`, has one entry per module
with `{name, bytes, disassembly}`:

- `bytes` — Base64-encoded raw bytecode (decoded into `.mv` for the optional decompile step)
- `disassembly` — text of the module's disassembly (byte-for-byte equivalent to what
  `move disassemble <file>.mv` would produce locally)

## 3. Write per-module .mv + .asm files

```sh
jq -c '.data.object.asMovePackage.modules.nodes[]' "$OUT/package.json" | while read -r mod; do
  name=$(jq -r '.name' <<<"$mod")
  jq -r '.bytes'       <<<"$mod" | base64 -d > "$OUT/mv/$name.mv"
  jq -r '.disassembly' <<<"$mod"               > "$OUT/asm/$name.asm"
done

# Magic-byte sanity (each .mv should start with a11ceb0b)
for f in "$OUT/mv/"*.mv; do
  head -c 4 "$f" | xxd -p | grep -q '^a11ceb0b' || echo "WARN: $f missing magic"
done
```

## 4. (Optional) Decompile for the human-explanation layer

```sh
move decompile --input "$OUT/mv" --output "$OUT/decompiled"
```

The decompiled `.move` is a heuristic reconstruction; see
`move-bytecode-comprehension/reading-decompiled.md` for the artifacts to recognize when
presenting findings to humans.

## Output paths

- `$OUT/mv/<module>.mv`             — raw bytecode
- `$OUT/asm/<module>.asm`           — disassembly (analysis substrate)
- `$OUT/decompiled/<package>/<module>.move`  — decompiled source (optional)
