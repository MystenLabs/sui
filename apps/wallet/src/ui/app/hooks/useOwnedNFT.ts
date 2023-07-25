// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetKioskContents, useGetObject } from '@mysten/core';
import { is, SuiObjectData, getObjectOwner } from '@mysten/sui.js';
import { useMemo } from 'react';

export function useOwnedNFT(nftObjectId: string | null, address: string | null) {
	const data = useGetObject(nftObjectId);
	const { data: kioskData, isFetching: areKioskContentsLoading } = useGetKioskContents(address);
	const { data: objectData, isLoading } = data;

	const objectDetails = useMemo(() => {
		if (!objectData || !is(objectData.data, SuiObjectData) || !address) return null;
		const ownedKioskObjectIds = kioskData?.list.map(({ data }) => data?.objectId) || [];
		const objectOwner = getObjectOwner(objectData);
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

	return { ...data, isLoading: isLoading || areKioskContentsLoading, data: objectDetails };
}
