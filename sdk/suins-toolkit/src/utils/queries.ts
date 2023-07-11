// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcProvider, SuiObjectResponse, getObjectOwner } from '@mysten/sui.js';

// get NFT's owner from RPC.
export const getOwner = async (
    provider: JsonRpcProvider,
    nftId: string,
): Promise<string | null> => {
    const ownerResponse = await provider.getObject({
        id: nftId,
        options: { showOwner: true },
    });
    const owner = getObjectOwner(ownerResponse);
    return (
        (owner as { AddressOwner: string })?.AddressOwner ||
        (owner as { ObjectOwner: string })?.ObjectOwner ||
        null
    );
};

// get avatar NFT Object from RPC.
export const getAvatar = async (
    provider: JsonRpcProvider,
    avatar: string,
): Promise<SuiObjectResponse> => {
    return await provider.getObject({
        id: avatar,
        options: {
            showDisplay: true,
            showOwner: true,
        },
    });
};
