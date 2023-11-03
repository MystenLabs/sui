// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useGetObject } from '@mysten/core';

export function useLotSize(poolId: string) {
	const { data } = useGetObject(poolId);
	const objectFields =
		data?.data?.content?.dataType === 'moveObject' ? data?.data?.content?.fields : null;

	return (objectFields as Record<string, string>)?.lot_size;
}
