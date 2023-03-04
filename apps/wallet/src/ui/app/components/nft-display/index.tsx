// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAddress } from '@mysten/sui.js';
import { cva, cx } from 'class-variance-authority';

import { Heading } from '_app/shared/heading';
import Loading from '_components/loading';
import { NftImage, type NftImageProps } from '_components/nft-display/NftImage';
import { useGetNFTMeta, useFileExtensionType, useOriginbyteNft } from '_hooks';

import type { VariantProps } from 'class-variance-authority';

const nftDisplayCardStyles = cva('flex flex-nowrap items-center h-full', {
    variants: {
        animateHover: {
            true: 'group',
        },
        wideView: {
            true: 'bg-gray-40 p-2.5 rounded-lg gap-2.5 flex-row-reverse justify-between',
            false: 'flex-col',
        },
    },
    defaultVariants: {
        wideView: false,
    },
});

export interface NFTsProps extends VariantProps<typeof nftDisplayCardStyles> {
    objectId: string;
    showLabel?: boolean;
    size: NftImageProps['size'];
    borderRadius?: NftImageProps['borderRadius'];
}

export function NFTDisplayCard({
    objectId,
    showLabel,
    size,
    wideView,
    animateHover,
    borderRadius = 'md',
}: NFTsProps) {
    const { data: nftMeta, isLoading } = useGetNFTMeta(objectId);
    const { data: originByteNft, isLoading: originByteLoading } =
        useOriginbyteNft(objectId);
    const nftName =
        typeof nftMeta?.name === 'string'
            ? nftMeta?.name
            : formatAddress(objectId);

    // display title is the either originByteNft field name or default nft name or fall back to it the object id
    const displayTitle = originByteNft?.fields.name || nftName;
    const nftUrl = nftMeta?.url;
    const fileExtensionType = useFileExtensionType(nftUrl!);
    return (
        <div className={nftDisplayCardStyles({ animateHover, wideView })}>
            <Loading loading={isLoading || originByteLoading}>
                <NftImage
                    name={originByteNft?.fields.name || nftName!}
                    src={originByteNft?.fields.url || nftUrl}
                    title={
                        originByteNft?.fields.description ||
                        nftMeta?.description ||
                        ''
                    }
                    animateHover={true}
                    showLabel={!wideView}
                    borderRadius={borderRadius}
                    size={size}
                />
                {wideView ? (
                    <div className="flex flex-col gap-1 flex-1 min-w-0 ml-1">
                        <Heading variant="heading6" color="gray-90" truncate>
                            {displayTitle}
                        </Heading>

                        <div className="text-gray-75 text-body font-medium">
                            {nftMeta?.url ? (
                                `${fileExtensionType.name} ${fileExtensionType.type}`
                            ) : (
                                <span className="uppercase font-normal text-bodySmall">
                                    NO MEDIA
                                </span>
                            )}
                        </div>
                    </div>
                ) : showLabel ? (
                    <div
                        className={cx(
                            'flex-1 mt-2 text-steel-dark truncate overflow-hidden max-w-full',
                            animateHover
                                ? 'group-hover:text-black duration-200 ease-ease-in-out-cubic'
                                : ''
                        )}
                    >
                        {displayTitle}
                    </div>
                ) : null}
            </Loading>
        </div>
    );
}
