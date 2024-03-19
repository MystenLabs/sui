// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetKioskContents, useGetObject } from '@mysten/core';
import { useMemo } from 'react';

export function useOwnedNFT(nftObjectId: string | null, address: string | null) {
	const data = useGetObject(nftObjectId);
	const { data: kioskData, isFetching: areKioskContentsLoading } = useGetKioskContents(address);
	const { data: objectData, isPending } = data;

	const objectDetails = useMemo(() => {
		if (!objectData || !objectData.data || !address) return null;
		const ownedKioskObjectIds = kioskData?.list.map(({ data }) => data?.objectId) || [];
		const objectOwner = objectData.data.owner;
		const data =
			ownedKioskObjectIds.includes(objectData.data.objectId) ||
			(objectOwner &&
				objectOwner !== 'Immutable' &&
				'AddressOwner' in objectOwner &&
				objectOwner.AddressOwner === address)
				? objectData.data
				: null;
		return data;
	}, [address, objectData, kioskData]);

	return { ...data, isPending: isPending || areKioskContentsLoading, data: objectDetails };
}
