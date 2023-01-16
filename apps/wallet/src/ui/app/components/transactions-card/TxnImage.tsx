// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '_app/shared/text';
import { useGetNFTMeta, useMiddleEllipsis } from '_hooks';

const TRUNCATE_MAX_CHAR = 34;

//TODO merge all NFT image displays
export function TxnImage({ id }: { id: string }) {
    const nftMeta = useGetNFTMeta(id);
    const name = useMiddleEllipsis(
        nftMeta?.name || '',
        TRUNCATE_MAX_CHAR,
        TRUNCATE_MAX_CHAR - 1
    );

    const description = useMiddleEllipsis(
        nftMeta?.description || '',
        TRUNCATE_MAX_CHAR,
        TRUNCATE_MAX_CHAR - 1
    );

    return nftMeta ? (
        <div className="flex w-full gap-2">
            <img
                src={nftMeta.url.replace(/^ipfs:\/\//, 'https://ipfs.io/ipfs/')}
                className="w-10 h-10 rounded"
                alt={nftMeta.name || 'Nft image'}
            />
            <div className="flex flex-col gap-1">
                <Text color="gray-90" weight="semibold" variant="subtitleSmall">
                    {name}
                </Text>
                <Text
                    color="steel-darker"
                    weight="medium"
                    variant="subtitleSmall"
                >
                    {description}
                </Text>
            </div>
        </div>
    ) : null;
}
