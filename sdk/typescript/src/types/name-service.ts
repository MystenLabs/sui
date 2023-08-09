// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Infer } from 'superstruct';
import { array, boolean, nullable, object, string } from 'superstruct';

export const ResolvedNameServiceNames = object({
	data: array(string()),
	hasNextPage: boolean(),
	nextCursor: nullable(string()),
});
export type ResolvedNameServiceNames = Infer<typeof ResolvedNameServiceNames>;
