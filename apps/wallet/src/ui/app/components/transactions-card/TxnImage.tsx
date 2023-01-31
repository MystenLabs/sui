// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '_app/shared/text';
import { useGetNFTMeta } from '_hooks';

//TODO merge all NFT image displays
export function TxnImage({ id, label }: { id: string; label?: string }) {
    const nftMeta = useGetNFTMeta(id);

    return nftMeta?.url ? (
        <div className="flex gap-2 flex-col">
            {label && (
                <Text variant="body" weight="medium" color="steel-darker">
                    {label}
                </Text>
            )}
            <div className="flex w-full gap-2">
                <div className="flex gap-2">
                    <img
                        src={nftMeta.url.replace(
                            /^ipfs:\/\//,
                            'https://ipfs.io/ipfs/'
                        )}
                        className="w-10 h-10 rounded"
                        alt={nftMeta.name || 'Nft image'}
                    />
                </div>
                <div className="flex flex-col gap-1 justify-center break-all w-56">
                    {nftMeta.name && (
                        <Text
                            color="gray-90"
                            weight="semibold"
                            variant="subtitleSmall"
                            truncate
                        >
                            {nftMeta.name}
                        </Text>
                    )}
                    {nftMeta.description && (
                        <Text
                            color="steel-darker"
                            weight="medium"
                            variant="subtitleSmall"
                            truncate
                        >
                            {nftMeta.description}
                        </Text>
                    )}
                </div>
            </div>
        </div>
    ) : null;
}
