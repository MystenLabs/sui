// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export type SuiMoveFunctionArgType = string | { Object: string };

export type SuiMoveNormalizedFunction = {
	visibility: SuiMoveVisibility;
	isEntry: boolean;
	typeParameters: SuiMoveAbilitySet[];
	parameters: SuiMoveNormalizedType[];
	return: SuiMoveNormalizedType[];
};

export type SuiMoveNormalizedType =
	| string
	| SuiMoveNormalizedTypeParameterType
	| { Reference: SuiMoveNormalizedType }
	| { MutableReference: SuiMoveNormalizedType }
	| { Vector: SuiMoveNormalizedType }
	| SuiMoveNormalizedStructType;

export type SuiMoveNormalizedStructType = {
	Struct: {
		address: string;
		module: string;
		name: string;
		typeArguments: SuiMoveNormalizedType[];
	};
};

export type SuiMoveAbilitySet = {
	abilities: string[];
};

export type SuiMoveNormalizedTypeParameterType = {
	TypeParameter: number;
};

export type SuiMoveVisibility = 'Private' | 'Public' | 'Friend';

export type SuiMoveNormalizedModule = {
	fileFormatVersion: number;
	address: string;
	name: string;
	friends: SuiMoveModuleId[];
	structs: Record<string, SuiMoveNormalizedStruct>;
	exposedFunctions: Record<string, SuiMoveNormalizedFunction>;
};

export type SuiMoveModuleId = {
	address: string;
	name: string;
};

export type SuiMoveNormalizedModules = Record<string, SuiMoveNormalizedModule>;

export type SuiMoveNormalizedStruct = {
	abilities: SuiMoveAbilitySet;
	typeParameters: SuiMoveStructTypeParameter[];
	fields: SuiMoveNormalizedField[];
};

export type SuiMoveStructTypeParameter = {
	constraints: SuiMoveAbilitySet;
	isPhantom: boolean;
};

export type SuiMoveNormalizedField = {
	name: string;
	type: SuiMoveNormalizedType;
};

export type SuiMovePackage = {
	/** A mapping from module name to disassembled Move bytecode */
	disassembled: MovePackageContent;
};

export type MovePackageContent = Record<string, string>;

export type MoveCallMetrics = {
	rank3Days: MoveCallMetric[];
	rank7Days: MoveCallMetric[];
	rank30Days: MoveCallMetric[];
};

export type MoveCallMetric = [
	{
		module: string;
		package: string;
		function: string;
	},
	string,
];
