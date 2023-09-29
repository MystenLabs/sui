// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Infer } from 'superstruct';
import {
	array,
	boolean,
	define,
	is,
	literal,
	number,
	object,
	record,
	string,
	tuple,
	union,
} from 'superstruct';

export type SuiMoveFunctionArgTypesResponse = Infer<typeof SuiMoveFunctionArgType>[];

export const SuiMoveFunctionArgType = union([string(), object({ Object: string() })]);

export const SuiMoveFunctionArgTypes = array(SuiMoveFunctionArgType);
export type SuiMoveFunctionArgTypes = Infer<typeof SuiMoveFunctionArgTypes>;

export const SuiMoveModuleId = object({
	address: string(),
	name: string(),
});
export type SuiMoveModuleId = Infer<typeof SuiMoveModuleId>;

export const SuiMoveVisibility = union([literal('Private'), literal('Public'), literal('Friend')]);
export type SuiMoveVisibility = Infer<typeof SuiMoveVisibility>;

export const SuiMoveAbilitySet = object({
	abilities: array(string()),
});
export type SuiMoveAbilitySet = Infer<typeof SuiMoveAbilitySet>;

export const SuiMoveStructTypeParameter = object({
	constraints: SuiMoveAbilitySet,
	isPhantom: boolean(),
});
export type SuiMoveStructTypeParameter = Infer<typeof SuiMoveStructTypeParameter>;

export const SuiMoveNormalizedTypeParameterType = object({
	TypeParameter: number(),
});
export type SuiMoveNormalizedTypeParameterType = Infer<typeof SuiMoveNormalizedTypeParameterType>;

export type SuiMoveNormalizedType =
	| string
	| SuiMoveNormalizedTypeParameterType
	| { Reference: SuiMoveNormalizedType }
	| { MutableReference: SuiMoveNormalizedType }
	| { Vector: SuiMoveNormalizedType }
	| SuiMoveNormalizedStructType;

export const MoveCallMetric = tuple([
	object({
		module: string(),
		package: string(),
		function: string(),
	}),
	string(),
]);

export type MoveCallMetric = Infer<typeof MoveCallMetric>;

export const MoveCallMetrics = object({
	rank3Days: array(MoveCallMetric),
	rank7Days: array(MoveCallMetric),
	rank30Days: array(MoveCallMetric),
});

export type MoveCallMetrics = Infer<typeof MoveCallMetrics>;

function isSuiMoveNormalizedType(value: unknown): value is SuiMoveNormalizedType {
	if (!value) return false;
	if (typeof value === 'string') return true;
	if (is(value, SuiMoveNormalizedTypeParameterType)) return true;
	if (isSuiMoveNormalizedStructType(value)) return true;
	if (typeof value !== 'object') return false;

	const valueProperties = value as Record<string, unknown>;
	if (is(valueProperties.Reference, SuiMoveNormalizedType)) return true;
	if (is(valueProperties.MutableReference, SuiMoveNormalizedType)) return true;
	if (is(valueProperties.Vector, SuiMoveNormalizedType)) return true;
	return false;
}

export const SuiMoveNormalizedType = define<SuiMoveNormalizedType>(
	'SuiMoveNormalizedType',
	isSuiMoveNormalizedType,
);

export type SuiMoveNormalizedStructType = {
	Struct: {
		address: string;
		module: string;
		name: string;
		typeArguments: SuiMoveNormalizedType[];
	};
};

function isSuiMoveNormalizedStructType(value: unknown): value is SuiMoveNormalizedStructType {
	if (!value || typeof value !== 'object') return false;

	const valueProperties = value as Record<string, unknown>;
	if (!valueProperties.Struct || typeof valueProperties.Struct !== 'object') return false;

	const structProperties = valueProperties.Struct as Record<string, unknown>;
	if (
		typeof structProperties.address !== 'string' ||
		typeof structProperties.module !== 'string' ||
		typeof structProperties.name !== 'string' ||
		!Array.isArray(structProperties.typeArguments) ||
		!structProperties.typeArguments.every((value) => isSuiMoveNormalizedType(value))
	) {
		return false;
	}

	return true;
}

// NOTE: This type is recursive, so we need to manually implement it:
export const SuiMoveNormalizedStructType = define<SuiMoveNormalizedStructType>(
	'SuiMoveNormalizedStructType',
	isSuiMoveNormalizedStructType,
);

export const SuiMoveNormalizedFunction = object({
	visibility: SuiMoveVisibility,
	isEntry: boolean(),
	typeParameters: array(SuiMoveAbilitySet),
	parameters: array(SuiMoveNormalizedType),
	return: array(SuiMoveNormalizedType),
});
export type SuiMoveNormalizedFunction = Infer<typeof SuiMoveNormalizedFunction>;

export const SuiMoveNormalizedField = object({
	name: string(),
	type: SuiMoveNormalizedType,
});
export type SuiMoveNormalizedField = Infer<typeof SuiMoveNormalizedField>;

export const SuiMoveNormalizedStruct = object({
	abilities: SuiMoveAbilitySet,
	typeParameters: array(SuiMoveStructTypeParameter),
	fields: array(SuiMoveNormalizedField),
});
export type SuiMoveNormalizedStruct = Infer<typeof SuiMoveNormalizedStruct>;

export const SuiMoveNormalizedModule = object({
	fileFormatVersion: number(),
	address: string(),
	name: string(),
	friends: array(SuiMoveModuleId),
	structs: record(string(), SuiMoveNormalizedStruct),
	exposedFunctions: record(string(), SuiMoveNormalizedFunction),
});
export type SuiMoveNormalizedModule = Infer<typeof SuiMoveNormalizedModule>;

export const SuiMoveNormalizedModules = record(string(), SuiMoveNormalizedModule);
export type SuiMoveNormalizedModules = Infer<typeof SuiMoveNormalizedModules>;

export function extractMutableReference(
	normalizedType: SuiMoveNormalizedType,
): SuiMoveNormalizedType | undefined {
	return typeof normalizedType === 'object' && 'MutableReference' in normalizedType
		? normalizedType.MutableReference
		: undefined;
}

export function extractReference(
	normalizedType: SuiMoveNormalizedType,
): SuiMoveNormalizedType | undefined {
	return typeof normalizedType === 'object' && 'Reference' in normalizedType
		? normalizedType.Reference
		: undefined;
}

export function extractStructTag(
	normalizedType: SuiMoveNormalizedType,
): SuiMoveNormalizedStructType | undefined {
	if (typeof normalizedType === 'object' && 'Struct' in normalizedType) {
		return normalizedType;
	}

	const ref = extractReference(normalizedType);
	const mutRef = extractMutableReference(normalizedType);

	if (typeof ref === 'object' && 'Struct' in ref) {
		return ref;
	}

	if (typeof mutRef === 'object' && 'Struct' in mutRef) {
		return mutRef;
	}
	return undefined;
}
