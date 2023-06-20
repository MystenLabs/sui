// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export default function notEmpty<TValue>(value: TValue | null | undefined): value is TValue {
	if (value === null || value === undefined) return false;
	return true;
}
