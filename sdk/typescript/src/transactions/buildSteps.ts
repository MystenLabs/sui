// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { parse } from 'valibot';

import type { BcsType } from '../bcs/index.js';
import { bcs } from '../bcs/index.js';
import { normalizeSuiAddress, normalizeSuiObjectId } from '../utils/index.js';
import type { Argument, CallArg, OpenMoveTypeSignature, Transaction } from './blockData/v2.js';
import { ObjectRef } from './blockData/v2.js';
import { Inputs, isMutableSharedObjectInput } from './Inputs.js';
import { getPureBcsSchema, isTxContext } from './serializer.js';
import type { TransactionBlockDataBuilder } from './TransactionBlockData.js';
import type { TransactionBlockDataResolver } from './TransactionBlockDataResolver.js';

export async function setGasPrice(
	blockData: TransactionBlockDataBuilder,
	dataResolver: TransactionBlockDataResolver,
) {
	if (!blockData.gasConfig.price) {
		blockData.gasConfig.price = String(await dataResolver.getGasPrice(blockData));
	}
}

export async function setGasBudget(
	blockData: TransactionBlockDataBuilder,
	dataResolver: TransactionBlockDataResolver,
) {
	if (!blockData.gasConfig.budget) {
		blockData.gasConfig.budget = String(await dataResolver.getGasBudget(blockData));
	}
}

// The current default is just picking _all_ coins we can which may not be ideal.
export async function setGasPayment(
	blockData: TransactionBlockDataBuilder,
	dataResolver: TransactionBlockDataResolver,
) {
	if (blockData.gasConfig.payment) {
		const maxGasObjects = dataResolver.getLimit('maxGasObjects');
		if (blockData.gasConfig.payment.length > maxGasObjects) {
			throw new Error(`Payment objects exceed maximum amount: ${maxGasObjects}`);
		}
		const paymentCoins = await dataResolver.getGasCoins(
			blockData,
			blockData.gasConfig.owner || blockData.sender!,
		);
		blockData.gasConfig.payment = paymentCoins.map((payment) => parse(ObjectRef, payment));
	}
}

export async function resolveObjectReferences(
	blockData: TransactionBlockDataBuilder,
	dataResolver: TransactionBlockDataResolver,
) {
	// Keep track of the object references that will need to be resolved at the end of the transaction.
	// We keep the input by-reference to avoid needing to re-resolve it:
	const objectsToResolve = blockData.inputs.filter((input) => {
		return input.UnresolvedObject;
	}) as Extract<CallArg, { UnresolvedObject: unknown }>[];

	if (objectsToResolve.length) {
		const dedupedIds = [
			...new Set(
				objectsToResolve.map((input) => normalizeSuiObjectId(input.UnresolvedObject.value)),
			),
		];

		const objects = await dataResolver.getObjects(dedupedIds);

		let objectsById = new Map(
			dedupedIds.map((id, index) => {
				return [id, objects[index]];
			}),
		);

		objectsToResolve.forEach((input) => {
			let updated: CallArg | undefined;
			const id = normalizeSuiAddress(input.UnresolvedObject.value);
			const typeSignatures = input.UnresolvedObject.typeSignatures;
			const object = objectsById.get(id)!;

			const isMutable = typeSignatures.some((typeSignature) => {
				// There could be multiple transactions that reference the same shared object.
				// If one of them is a mutable reference or taken by value, then we should mark the input
				// as mutable.
				const isByValue = !typeSignature.ref;
				return isMutableSharedObjectInput(input) || isByValue || typeSignature.ref === '&mut';
			});
			const isReceiving = !object.initialSharedVersion && typeSignatures.some(isReceivingType);

			if (object.initialSharedVersion) {
				updated = Inputs.SharedObjectRef({
					objectId: id,
					initialSharedVersion: object.initialSharedVersion,
					mutable: isMutable,
				});
			} else if (isReceiving) {
				updated = Inputs.ReceivingRef(
					{
						objectId: id,
						digest: object.digest,
						version: object.version,
					}!,
				);
			}

			blockData.inputs[blockData.inputs.indexOf(input)] =
				updated ??
				Inputs.ObjectRef({
					objectId: id,
					digest: object.digest,
					version: object.version,
				});
		});
	}
}

export async function normalizeInputs(
	blockData: TransactionBlockDataBuilder,
	dataResolver: TransactionBlockDataResolver,
) {
	const { inputs, transactions } = blockData;
	const moveModulesToResolve: Extract<Transaction, { MoveCall: unknown }>['MoveCall'][] = [];

	transactions.forEach((transaction) => {
		// Special case move call:
		if (transaction.MoveCall) {
			// Determine if any of the arguments require encoding.
			// - If they don't, then this is good to go.
			// - If they do, then we need to fetch the normalized move module.

			const inputs = transaction.MoveCall.arguments.map((arg) => {
				if (arg.$kind === 'Input') {
					return blockData.inputs[arg.Input];
				}
				return null;
			});
			const needsResolution = inputs.some(
				(input) => input && (input.RawValue || input.UnresolvedObject),
			);

			if (needsResolution) {
				moveModulesToResolve.push(transaction.MoveCall);
			}
		}

		// Special handling for values that where previously encoded using the wellKnownEncoding pattern.
		// This should only happen when transaction block data was hydrated from an old version of the SDK
		switch (transaction.$kind) {
			case 'SplitCoins':
				transaction.SplitCoins[1].forEach((amount) => {
					normalizeRawArgument(amount, bcs.U64, blockData);
				});
				break;
			case 'TransferObjects':
				normalizeRawArgument(transaction.TransferObjects[1], bcs.Address, blockData);
				break;
		}
	});

	if (moveModulesToResolve.length) {
		await Promise.all(
			moveModulesToResolve.map(async (moveCall) => {
				const normalized = await dataResolver.getMoveFunctionDefinition({
					package: moveCall.package,
					module: moveCall.module,
					function: moveCall.function,
				});

				// Entry functions can have a mutable reference to an instance of the TxContext
				// struct defined in the TxContext module as the last parameter. The caller of
				// the function does not need to pass it in as an argument.
				const hasTxContext =
					normalized.parameters.length > 0 && isTxContext(normalized.parameters.at(-1)!);

				const params = hasTxContext
					? normalized.parameters.slice(0, normalized.parameters.length - 1)
					: normalized.parameters;

				if (params.length !== moveCall.arguments.length) {
					throw new Error('Incorrect number of arguments.');
				}

				params.forEach((param, i) => {
					const arg = moveCall.arguments[i];
					if (arg.$kind !== 'Input') return;
					const input = inputs[arg.Input];
					// Skip if the input is already resolved
					if (!input.RawValue && !input.UnresolvedObject) return;

					const inputValue = input.RawValue?.value ?? input.UnresolvedObject?.value!;

					const schema = getPureBcsSchema(param.body);
					if (schema) {
						inputs[inputs.indexOf(input)] = Inputs.Pure(schema.serialize(inputValue));
						return;
					}

					if (typeof inputValue !== 'string') {
						throw new Error(
							`Expect the argument to be an object id string, got ${JSON.stringify(
								inputValue,
								null,
								2,
							)}`,
						);
					}

					if (input.$kind === 'RawValue') {
						inputs[inputs.indexOf(input)] = {
							$kind: 'UnresolvedObject',
							UnresolvedObject: {
								value: inputValue,
								typeSignatures: [param],
							},
						};
					} else {
						input.UnresolvedObject.typeSignatures.push(param);
					}
				});
			}),
		);
	}
}

export async function validate(
	blockData: TransactionBlockDataBuilder,
	dataResolver: TransactionBlockDataResolver,
) {
	blockData.inputs.forEach((input, index) => {
		if (input.Pure) {
			const maxPureArgumentSize = dataResolver.getLimit('maxPureArgumentSize');
			if (input.Pure.length > maxPureArgumentSize) {
				throw new Error(
					`Input at index ${index} is too large, max pure input size is ${maxPureArgumentSize} bytes, got ${input.Pure.length} bytes`,
				);
			}
		}

		if (input.$kind !== 'Object' && input.$kind !== 'Pure') {
			throw new Error(
				`Input at index ${index} has not been resolved.  Expected a Pure or Object input, but found ${JSON.stringify(
					input,
				)}`,
			);
		}
	});
}

function normalizeRawArgument(
	arg: Argument,
	schema: BcsType<any>,
	blockData: TransactionBlockDataBuilder,
) {
	if (arg.$kind !== 'Input') {
		return;
	}
	const input = blockData.inputs[arg.Input];

	if (input.$kind !== 'RawValue') {
		return;
	}

	blockData.inputs[arg.Input] = Inputs.Pure(schema.serialize(input.RawValue.value));
}

function isReceivingType(type: OpenMoveTypeSignature): boolean {
	if (typeof type.body !== 'object' || !('datatype' in type.body)) {
		return false;
	}

	return (
		type.body.datatype.package === '0x2' &&
		type.body.datatype.module === 'transfer' &&
		type.body.datatype.type === 'Receiving'
	);
}
