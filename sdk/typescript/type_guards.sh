#!/bin/zsh

# generate TS type guards for project
npx ts-auto-guard --export-all src/rpc/client.ts src/**.ts

second_arg=", _argumentName?: string"
if [[ `uname` == "Darwin" ]]; then
    # fix import of BN.js types
    sed -i '' 's/"..\/node_modules\/@types\/bn";/"bn.js";/g' src/index.guard.ts
    # remove the unsupported second argument from generated type guards
    sed -i '' "s/${second_arg}//g" src/index.guard.ts
    sed -i '' "s/${second_arg}//g" src/rpc/client.guard.ts
else
    # linux / GNU sed versions of same thing
    sed -i 's/"..\/node_modules\/@types\/bn";/"bn.js";/g' src/index.guard.ts
    sed -i "s/${second_arg}//g" src/index.guard.ts
    sed -i "s/${second_arg}//g" src/rpc/client.guard.ts
fi

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