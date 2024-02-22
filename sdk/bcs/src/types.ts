// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { BcsType } from './bcs-type.js';

/**
 * Supported encodings.
 * Used in `Reader.toString()` as well as in `decodeStr` and `encodeStr` functions.
 */
export type Encoding = 'base58' | 'base64' | 'hex';

type UnionToIntersection<T> = (T extends any ? (x: T) => any : never) extends (x: infer R) => any
	? { [K in keyof R]: R[K] }
	: never;

type RecursivelyReplacePlaceholder<
	T,
	R extends Record<string, unknown>,
> = T extends GenericPlaceholder<infer K extends keyof R>
	? R[K]
	: T extends Record<string, unknown> | unknown[]
	? { [K in keyof T]: RecursivelyReplacePlaceholder<T[K], R> }
	: T extends Map<infer K, infer V>
	? Map<RecursivelyReplacePlaceholder<K, R>, RecursivelyReplacePlaceholder<V, R>>
	: T;

const bcsGenericPlaceholder = Symbol('bcsPlaceholder');

export interface GenericPlaceholder<T> {
	[bcsGenericPlaceholder]: T;
}

export type ReplaceBcsGenerics<
	Type extends BcsType<any>,
	Names extends readonly string[],
	Types extends { [K in keyof Names]: BcsType<any> },
> = Type extends BcsType<infer U, any>
	? BcsType<
			RecursivelyReplacePlaceholder<
				U,
				UnionToIntersection<
					{
						[K in keyof Names]: Types[K] extends BcsType<infer R, any>
							? { [K2 in Names[K]]: R }
							: never;
					}[number]
				>
			>,
			RecursivelyReplacePlaceholder<
				U,
				UnionToIntersection<
					{
						[K in keyof Names]: Types[K] extends BcsType<any, infer R>
							? { [K2 in Names[K]]: R }
							: never;
					}[number]
				>
			>
	  >
	: never;

export type InferBcsType<T extends BcsType<any>> = T extends BcsType<infer U, any> ? U : never;
export type InferBcsInput<T extends BcsType<any, any>> = T extends BcsType<any, infer U>
	? U
	: never;

type EnumValue<T, Keys extends string> = T & { [K in Exclude<Keys, keyof T>]?: never };

export type EnumOutputShape<
	T extends Record<string, BcsType<any> | null>,
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

export type EnumOutputShapeWithKeys<
	T extends Record<string, BcsType<any> | null>,
	Keys extends string,
> = {
	[K in keyof T]: EnumValue<
		T[K] extends BcsType<infer U, any> ? { [K2 in K]: U } : { [K2 in K]: true },
		Keys
	> & {
		$kind: K;
	};
}[keyof T];

export type EnumInputShape<T extends Record<string, BcsType<any> | null>> = {
	[K in keyof T]: T[K] extends BcsType<any, infer U>
		? { [K2 in K]: U }
		: { [K2 in K]: boolean | object | null };
}[keyof T];
