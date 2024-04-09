// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '_src/ui/app/shared/text';
import { ChevronDown12, ChevronRight12 } from '@mysten/icons';
import { TypeTagSerializer, type TypeTag } from '@mysten/sui.js/bcs';
import {
	type TransactionArgument,
	type TransactionBlockState,
	type Transactions,
} from '@mysten/sui.js/transactions';
import { toB64 } from '@mysten/sui.js/utils';
import { useState } from 'react';

type TransactionType = TransactionBlockState['transactions'][0];
type MakeMoveVecTransaction = ReturnType<(typeof Transactions)['MakeMoveVec']>;
type PublishTransaction = ReturnType<(typeof Transactions)['Publish']>;
type Argument = Exclude<TransactionArgument, (...args: any) => unknown>;

function convertCommandArgumentToString(
	arg:
		| null
		| string
		| number
		| string[]
		| number[]
		| Argument
		| Argument[]
		| MakeMoveVecTransaction['MakeMoveVec'][0]
		| PublishTransaction['Publish'][0],
): string | null {
	if (!arg) return null;

	if (typeof arg === 'string' || typeof arg === 'number') return String(arg);

	if (typeof arg === 'object' && 'None' in arg) {
		return null;
	}

	if (typeof arg === 'object' && 'Some' in arg) {
		if (typeof arg.Some === 'object') {
			// MakeMoveVecTransaction['type'] is TypeTag type
			return TypeTagSerializer.tagToString(arg.Some as TypeTag);
		}
		return arg;
	}

	if (Array.isArray(arg)) {
		// Publish transaction special casing:
		if (typeof arg[0] === 'number') {
			return toB64(new Uint8Array(arg as number[]));
		}

		return `[${arg.map((argVal) => convertCommandArgumentToString(argVal)).join(', ')}]`;
	}

	switch (arg.$kind) {
		case 'GasCoin':
			return 'GasCoin';
		case 'Input':
			return `Input(${arg.Input})`;
		case 'Result':
			return `Result(${arg.Result})`;
		case 'NestedResult':
			return `NestedResult(${arg.NestedResult[0]}, ${arg.NestedResult[1]})`;
		default:
			// eslint-disable-next-line no-console
			console.warn('Unexpected command argument type.', arg);
			return null;
	}
}

function convertCommandToString(command: TransactionType) {
	let normalizedCommand;
	switch (command.$kind) {
		case 'MoveCall':
			normalizedCommand = {
				kind: 'MoveCall',
				...command.MoveCall,
				typeArguments: command.MoveCall.typeArguments.map((typeArg) =>
					TypeTagSerializer.tagToString(typeArg),
				),
			};
			break;
		case 'MakeMoveVec':
			normalizedCommand = {
				kind: 'MakeMoveVec',
				type: command.MakeMoveVec[0].Some
					? TypeTagSerializer.tagToString(command.MakeMoveVec[0].Some)
					: null,
				objects: command.MakeMoveVec[1],
			};
			break;
		case 'MergeCoins':
			normalizedCommand = {
				kind: 'MergeCoins',
				destination: command.MergeCoins[0],
				sources: command.MergeCoins[1],
			};
			break;
		case 'TransferObjects':
			normalizedCommand = {
				kind: 'TransferObjects',
				objects: command.TransferObjects[0],
				address: command.TransferObjects[1],
			};
			break;
		case 'SplitCoins':
			normalizedCommand = {
				kind: 'SplitCoins',
				coin: command.SplitCoins[0],
				amounts: command.SplitCoins[1],
			};
			break;
		case 'Publish':
			normalizedCommand = {
				kind: 'Publish',
				module: command.Publish[0],
				dependencies: command.Publish[1],
			};
			break;
		case 'Upgrade':
			normalizedCommand = {
				kind: 'Upgrade',
				modules: command.Upgrade[0],
				dependencies: command.Upgrade[1],
				packageId: command.Upgrade[2],
				ticket: command.Upgrade[3],
			};
			break;
		case 'TransactionIntent': {
			throw new Error('TransactionIntent is not supported');
		}
	}

	const commandArguments = Object.entries(normalizedCommand);
	return commandArguments
		.map(([key, value]) => {
			const stringValue = convertCommandArgumentToString(value);

			if (!stringValue) return null;

			return `${key}: ${stringValue}`;
		})
		.filter(Boolean)
		.join(', ');
}

interface CommandProps {
	command: TransactionType;
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
					{command.$kind}
				</Text>
				<div className="h-px bg-gray-40 flex-1" />
				<div className="text-steel">{expanded ? <ChevronDown12 /> : <ChevronRight12 />}</div>
			</button>

			{expanded && (
				<div className="mt-2 text-pBodySmall font-medium text-steel">
					({convertCommandToString(command)})
				</div>
			)}
		</div>
	);
}
