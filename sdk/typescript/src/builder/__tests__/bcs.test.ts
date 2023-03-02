// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { it, expect } from 'vitest';
import { builder, PROGRAMMABLE_CALL, MoveCallCommand, ENUM_KIND, COMMAND, TransferObjectsCommand } from '..';

// Oooh-weeee we nailed it!
it('can serialize simplified programmable call struct', () => {
    const moveCall: MoveCallCommand = {
        kind: 'MoveCall',
        target: "0x2::display::new",
        typeArguments: [ "0x6::capy::Capy" ],
        arguments: [
            { kind: 'GasCoin' },
            {
                kind: 'NestedResult',
                index: 0,
                resultIndex: 1
            },
            { kind: 'Input', index: 3 },
            { kind: 'Result', index: 1 }
        ],
    };

    const bytes = builder.ser(PROGRAMMABLE_CALL, moveCall).toBytes();
    const result: MoveCallCommand = builder.de(PROGRAMMABLE_CALL, bytes);

    // since we normalize addresses when (de)serializing, the returned value differs
    // only check the module and the function; ignore address comparison (it's not an issue
    // with non-0x2 addresses).
    expect(result.arguments).toEqual(moveCall.arguments);
    expect(result.target.split('::').slice(1)).toEqual(moveCall.target.split('::').slice(1));
    expect(result.typeArguments[0].split('::').slice(1)).toEqual(moveCall.typeArguments[0].split('::').slice(1));
});

it('can serialize enum with "kind" property', () => {
    const command = {
        kind: 'TransferObjects',
        objects: [],
        receiver: { kind: 'Input', index: 0 },
    };

    const bytes = builder.ser(COMMAND, command).toBytes();
    const result: TransferObjectsCommand = builder.de(COMMAND, bytes);

    expect(result).toEqual(command);
});
