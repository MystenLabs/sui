// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Infer } from 'superstruct';
import { any, array, boolean, literal, nullable, number, object, string, union } from 'superstruct';

export const DynamicFieldType = union([literal('DynamicField'), literal('DynamicObject')]);
export type DynamicFieldType = Infer<typeof DynamicFieldType>;

export const DynamicFieldName = object({
	type: string(),
	value: any(),
});
export type DynamicFieldName = Infer<typeof DynamicFieldName>;

export const DynamicFieldInfo = object({
	name: DynamicFieldName,
	bcsName: string(),
	type: DynamicFieldType,
	objectType: string(),
	objectId: string(),
	version: number(),
	digest: string(),
});
export type DynamicFieldInfo = Infer<typeof DynamicFieldInfo>;

export const DynamicFieldPage = object({
	data: array(DynamicFieldInfo),
	nextCursor: nullable(string()),
	hasNextPage: boolean(),
});
export type DynamicFieldPage = Infer<typeof DynamicFieldPage>;
