// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ChevronDown12, ChevronRight12 } from '@mysten/icons';
import {
	type TransactionType,
	type MakeMoveVecTransaction,
	type PublishTransaction,
	TypeTagSerializer,
	type TypeTag,
} from '@mysten/sui.js';
import { type TransactionArgument } from '@mysten/sui.js/transactions';
import { formatAddress, normalizeSuiAddress, toB64 } from '@mysten/sui.js/utils';

import { useState } from 'react';

import { Text } from '_src/ui/app/shared/text';

function convertCommandArgumentToString(
	arg:
		| string
		| number
		| string[]
		| number[]
		| TransactionArgument
		| TransactionArgument[]
		| MakeMoveVecTransaction['type']
		| PublishTransaction['modules'],
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
		return arg.Some;
	}

	if (Array.isArray(arg)) {
		// Publish transaction special casing:
		if (typeof arg[0] === 'number') {
			return toB64(new Uint8Array(arg as number[]));
		}

		return `[${arg.map((argVal) => convertCommandArgumentToString(argVal)).join(', ')}]`;
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
			// eslint-disable-next-line no-console
			console.warn('Unexpected command argument type.', arg);
			return null;
	}
}

function convertCommandToString({ kind, ...command }: TransactionType) {
	const commandArguments = Object.entries(command);

	return commandArguments
		.map(([key, value]) => {
			if (key === 'target') {
				const [packageId, moduleName, functionName] = value.split('::');
				return [
					`package: ${formatAddress(normalizeSuiAddress(packageId))}`,
					`module: ${moduleName}`,
					`function: ${functionName}`,
				].join(', ');
			}

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
					{command.kind}
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
