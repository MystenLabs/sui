// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Infer } from 'superstruct';
import { array, boolean, nullable, object, string } from 'superstruct';
import { ObjectId } from './common.js';

export const ResolvedNameServiceNames = object({
	data: array(string()),
	hasNextPage: boolean(),
	nextCursor: nullable(ObjectId),
});
export type ResolvedNameServiceNames = Infer<typeof ResolvedNameServiceNames>;
