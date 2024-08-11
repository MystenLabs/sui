// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { BcsType } from './bcs-type.js';

/**
 * Supported encodings.
 * Used in `Reader.toString()` as well as in `decodeStr` and `encodeStr` functions.
 */
export type Encoding = 'base58' | 'base64' | 'hex';

export type InferBcsType<T extends BcsType<any>> = T extends BcsType<infer U, any> ? U : never;
export type InferBcsInput<T extends BcsType<any, any>> =
	T extends BcsType<any, infer U> ? U : never;

type Merge<T> = T extends object ? { [K in keyof T]: T[K] } : never;
export type EnumOutputShape<
	T extends Record<string, unknown>,
	Keys extends string = Extract<keyof T, string>,
	Values = T[keyof T] extends infer Type ? (Type extends BcsType<infer U> ? U : never) : never,
> = 0 extends Values
	? EnumOutputShapeWithKeys<T, never>
	: 0n extends Values
		? EnumOutputShapeWithKeys<T, never>
		: '' extends Values
			? EnumOutputShapeWithKeys<T, never>
			: false extends Values
				? EnumOutputShapeWithKeys<T, never>
				: EnumOutputShapeWithKeys<T, Keys>;

export type EnumOutputShapeWithKeys<T extends Record<string, unknown>, Keys extends string> = {
	[K in keyof T]: Exclude<Keys, K> extends infer Empty extends string
		? Merge<
				{ [K2 in K]: T[K] } & { [K in Empty]?: never } & {
					$kind: K;
				}
			>
		: never;
}[keyof T];

export type EnumInputShape<T extends Record<string, unknown>> = {
	[K in keyof T]: { [K2 in K]: T[K] };
}[keyof T];
