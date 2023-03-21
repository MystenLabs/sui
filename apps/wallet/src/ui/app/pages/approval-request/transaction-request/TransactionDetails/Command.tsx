// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ChevronDown12, ChevronRight12 } from '@mysten/icons';
import {
    type CommandArgument,
    formatAddress,
    type TransactionCommand,
} from '@mysten/sui.js';
import { useState } from 'react';

import { Text } from '_src/ui/app/shared/text';

function convertCommandArgumentToString(
    arg: string | string[] | CommandArgument | CommandArgument[]
): string {
    if (typeof arg === 'string') return arg;

    if (Array.isArray(arg)) {
        return `[${arg
            .map((argVal) => convertCommandArgumentToString(argVal))
            .join(', ')}]`;
    }

    switch (arg.kind) {
        case 'GasCoin':
            return 'GasCoin';
        case 'Input':
            return `Input(${arg.index})`;
        case 'Result':
            return `Result(${arg.index})`;
        case 'NestedResult':
            return `NestedResult(${arg.index}, ${arg.resultIndex})`;
        default:
            throw new Error('Unexpected argument kind');
    }
}

function convertCommandToString({ kind, ...command }: TransactionCommand) {
    const commandArguments = Object.entries(command);

    return commandArguments
        .map(([key, value]) => {
            if (key === 'target') {
                const [packageId, moduleName, functionName] = value.split('::');
                return [
                    `package: ${formatAddress(packageId)}`,
                    `module: ${moduleName}`,
                    `function: ${functionName}`,
                ].join(', ');
            }

            return `${key}: ${convertCommandArgumentToString(value)}`;
        })
        .join(', ');
}

interface CommandProps {
    command: TransactionCommand;
}

export function Command({ command }: CommandProps) {
    const [expanded, setExpanded] = useState(true);

    return (
        <div>
            <button
                onClick={() => setExpanded((expanded) => !expanded)}
                className="flex items-center gap-2 w-full bg-transparent border-none p-0"
            >
                <Text variant="body" weight="semibold" color="steel-darker">
                    {command.kind}
                </Text>
                <div className="h-px bg-gray-40 flex-1" />
                <div className="text-steel">
                    {expanded ? <ChevronDown12 /> : <ChevronRight12 />}
                </div>
            </button>

            {expanded && (
                <div className="mt-2 text-p2 font-medium text-steel">
                    ({convertCommandToString(command)})
                </div>
            )}
        </div>
    );
}
