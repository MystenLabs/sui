#!/bin/zsh

# generate TS type guards for project
npx ts-auto-guard --export-all src/**.ts src/rpc/client.ts

# this only works on macos due to sed differences, perhaps a node script should do this?
# fix import of BN.js types on line 6
sed -i '' '6s/"..\/node_modules\/@types\/bn";/"bn.js";/g' src/index.guard.ts