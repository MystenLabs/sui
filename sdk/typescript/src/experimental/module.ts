// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    SuiObjectRef,
    CallArg,
    TypeTag,
    MoveCallTx,
} from './types';

export class MoveModule {
    constructor(public pkg: SuiObjectRef, public name: string) {}

    public call(
        method: string,
        type_args: TypeTag[],
        args: CallArg[]
    ): MoveCallTx {
        return {
            Call: {
                package: this.pkg,
                module: this.name,
                function: method,
                typeArguments: type_args,
                arguments: args,
            },
        };
    }
}
