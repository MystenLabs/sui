// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type BcsType } from './bcs-type';

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
