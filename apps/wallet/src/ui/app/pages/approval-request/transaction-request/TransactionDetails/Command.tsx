// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '_src/ui/app/shared/text';
import { ChevronDown12, ChevronRight12 } from '@mysten/icons';
import { type Argument, type Commands, type TransactionData } from '@mysten/sui/transactions';
import { toBase64 } from '@mysten/sui/utils';
import { useState } from 'react';

type TransactionType = TransactionData['commands'][0];
type MakeMoveVecTransaction = ReturnType<(typeof Commands)['MakeMoveVec']>;
type PublishTransaction = ReturnType<(typeof Commands)['Publish']>;

function convertCommandArgumentToString(
	arg:
		| null
		| string
		| number
		| string[]
		| number[]
		| Argument
		| Argument[]
		| MakeMoveVecTransaction['MakeMoveVec']['type']
		| PublishTransaction['Publish']['modules'],
): string | null {
	if (!arg) return null;

	if (typeof arg === 'string' || typeof arg === 'number') return String(arg);

	if (typeof arg === 'object' && 'None' in arg) {
		return null;
	}

	if (Array.isArray(arg)) {
		// Publish transaction special casing:
		if (typeof arg[0] === 'number') {
			return toBase64(new Uint8Array(arg as number[]));
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
				typeArguments: command.MoveCall.typeArguments,
			};
			break;
		case 'MakeMoveVec':
			normalizedCommand = {
				kind: 'MakeMoveVec',
				type: command.MakeMoveVec.type,
				elements: command.MakeMoveVec.elements,
			};
			break;
		case 'MergeCoins':
			normalizedCommand = {
				kind: 'MergeCoins',
				destination: command.MergeCoins.destination,
				sources: command.MergeCoins.sources,
			};
			break;
		case 'TransferObjects':
			normalizedCommand = {
				kind: 'TransferObjects',
				objects: command.TransferObjects.objects,
				address: command.TransferObjects.address,
			};
			break;
		case 'SplitCoins':
			normalizedCommand = {
				kind: 'SplitCoins',
				coin: command.SplitCoins.coin,
				amounts: command.SplitCoins.amounts,
			};
			break;
		case 'Publish':
			normalizedCommand = {
				kind: 'Publish',
				modules: command.Publish.modules,
				dependencies: command.Publish.dependencies,
			};
			break;
		case 'Upgrade':
			normalizedCommand = {
				kind: 'Upgrade',
				modules: command.Upgrade.modules,
				dependencies: command.Upgrade.dependencies,
				packageId: command.Upgrade.package,
				ticket: command.Upgrade.ticket,
			};
			break;
		case '$Intent': {
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
