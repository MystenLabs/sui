// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Struct } from 'superstruct';
import { create as superstructCreate } from 'superstruct';

export function create<T, S>(value: T, struct: Struct<T, S>): T {
	return superstructCreate(value, struct);
}
