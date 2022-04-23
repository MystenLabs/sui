#!/bin/zsh

# generate TS type guards for project
npx ts-auto-guard --export-all src/rpc/client.ts src/**.ts

LICENSE="// Copyright (c) 2022, Mysten Labs, Inc.\n// SPDX-License-Identifier: Apache-2.0\n";
second_arg=", _argumentName?: string"

for FILE in src/index.guard.ts src/rpc/client.guard.ts
do
    if [[ `uname` == "Darwin" ]]; then
        # fix import of BN.js types
        sed -i '' 's/"..\/node_modules\/@types\/bn";/"bn.js";/g' ${FILE}
        # remove the unsupported second argument from generated type guards
        sed -i '' "s/${second_arg}//g" ${FILE}
    else
        # linux / GNU sed versions of same thing
        sed -i 's/"..\/node_modules\/@types\/bn";/"bn.js";/g' ${FILE}
        sed -i "s/${second_arg}//g" ${FILE}
    fi

    TEMP=${FILE}.temp
    # add license header to generated file
    echo -e ${LICENSE} | cat - ${FILE} > ${TEMP}
    rm ${FILE}
    mv ${TEMP} ${FILE}
done
