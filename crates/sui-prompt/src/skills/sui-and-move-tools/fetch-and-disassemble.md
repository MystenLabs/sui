# Fetch bytecode + disassemble a deployed Sui package

Produces `.mv` (raw bytecode) and `.asm` (disassembled) files for every module in
the package. Disassembly is the working view for all reading and analysis.

## 1. Choose network + target

```sh
PKG=0x<package_id>
NETWORK=mainnet                 # or testnet, devnet
GQL="https://graphql.${NETWORK}.sui.io/graphql"
OUT="./.move-work/$PKG"
mkdir -p "$OUT/mv" "$OUT/asm"
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

**Note:** the query deliberately requests only `bytes`. Asking for `disassembly` here
pulls the full disassembly text for every module into `package.json`, which would leak
into the agent's context if the file is later inspected. We disassemble locally below
and write per-module `.asm` files instead.

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

## 4. Disassemble every module locally

```sh
for f in "$OUT/mv/"*.mv; do
  name=$(basename "$f" .mv)
  sui move disassemble "$f" > "$OUT/asm/$name.asm"
done
```

Each `.asm` is the full stack-machine disassembly of one module. See
`move-bytecode-comprehension/disassembly.md` for how to read it.

## Output paths

- `$OUT/mv/<module>.mv`    — raw bytecode
- `$OUT/asm/<module>.asm`  — disassembly (the working view)

## Reading disassembly surgically

A full `.asm` for a complex module can run tens of thousands of tokens. When inspecting,
`grep` / `sed` for the specific function or basic block rather than dumping the full file
into context.

```sh
# Locate a function by name
grep -n "^entry public " "$OUT/asm/<module>.asm"
grep -n "^public "       "$OUT/asm/<module>.asm"

# Read the surrounding block range once you have a line number
sed -n '<start>,<end>p' "$OUT/asm/<module>.asm"
```
