// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient, SuiObjectResponse } from '@mysten/sui/client';

// get NFT's owner from RPC.
export const getOwner = async (client: SuiClient, nftId: string): Promise<string | null> => {
	const ownerResponse = await client.getObject({
		id: nftId,
		options: { showOwner: true },
	});
	const owner = ownerResponse.data?.owner;
	return (
		(owner as { AddressOwner: string })?.AddressOwner ||
		(owner as { ObjectOwner: string })?.ObjectOwner ||
		null
	);
};

// get avatar NFT Object from RPC.
export const getAvatar = async (client: SuiClient, avatar: string): Promise<SuiObjectResponse> => {
	return await client.getObject({
		id: avatar,
		options: {
			showDisplay: true,
			showOwner: true,
		},
	});
};
