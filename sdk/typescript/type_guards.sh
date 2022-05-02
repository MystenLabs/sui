#!/bin/zsh
# Copyright (c) 2022, Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# generate TS type guards for project
npx ts-auto-guard --export-all src/rpc/client.ts src/**.ts

# this only works on macos due to sed differences, perhaps a node script should do this?
# fix import of BN.js types on line 6
sed -i '' '6s/"..\/node_modules\/@types\/bn";/"bn.js";/g' src/index.guard.ts

LICENSE="// Copyright (c) 2022, Mysten Labs, Inc.\n// SPDX-License-Identifier: Apache-2.0\n";
index="src/index.guard.ts"
# add license header to generated files
echo -e ${LICENSE} | cat - ${index} > src/index.guard.temp.ts
rm ${index}
mv src/index.guard.temp.ts ${index}

client="src/rpc/client.guard.ts"
echo -e ${LICENSE} | cat - ${client} > src/client.guard.temp.ts
rm ${client}
mv src/client.guard.temp.ts ${client}
