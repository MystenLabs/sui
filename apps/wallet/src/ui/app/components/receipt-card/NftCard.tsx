// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { cva, type VariantProps } from 'class-variance-authority';

import { Text } from '_app/shared/text';
import { useGetNFTMetaById, useMiddleEllipsis } from '_hooks';

const imageStyle = cva([], {
    variants: {
        size: {
            none: 'w-0 h-0',
            small: 'w-6 h-6',
            medium: 'w-icon h-icon',
            large: 'w-10 h-10',
        },
        variant: {
            rounded: 'rounded-full',
            square: 'rounded-sm',
        },
    },

    defaultVariants: {
        variant: 'rounded',
        size: 'medium',
    },
});

export interface NftMiniCardProps extends VariantProps<typeof imageStyle> {
    objectId: string;
    fnCallName?: string | null;
}

const TRUNCATE_MAX_CHAR = 40;

// TODO - merge this with the other NFT display component
export function NftMiniCard({
    objectId,
    fnCallName,
    ...styleProps
}: NftMiniCardProps) {
    const data = useGetNFTMetaById(objectId);
    const truncatedNftName = useMiddleEllipsis(
        data?.name || fnCallName || '',
        TRUNCATE_MAX_CHAR,
        TRUNCATE_MAX_CHAR - 1
    );

    const truncatedNftDescription = useMiddleEllipsis(
        data?.description || '',
        TRUNCATE_MAX_CHAR,
        TRUNCATE_MAX_CHAR - 1
    );

    return (
        data && (
            <div className="flex gap-2 items-center capitalize">
                <img
                    src={data.url.replace(
                        /^ipfs:\/\//,
                        'https://ipfs.io/ipfs/'
                    )}
                    className={imageStyle(styleProps)}
                    alt={data.name || 'NFT image'}
                />
                <div className="flex flex-col gap-1">
                    {truncatedNftName && (
                        <Text
                            variant="bodySmall"
                            color="gray-90"
                            weight="semibold"
                        >
                            {truncatedNftName}
                        </Text>
                    )}
                    {truncatedNftDescription && (
                        <Text
                            variant="subtitleSmall"
                            color="gray-80"
                            weight="normal"
                        >
                            {truncatedNftDescription}
                        </Text>
                    )}
                </div>
            </div>
        )
    );
}
