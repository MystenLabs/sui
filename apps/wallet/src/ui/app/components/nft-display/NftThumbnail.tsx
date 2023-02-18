// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva } from 'class-variance-authority';
import cl from 'classnames';

import Loading from '_components/loading';
import { NftImage, type NftImageProps } from '_components/nft-display/NftImage';
import { useGetNFTMeta, useMiddleEllipsis, useFileExtensionType } from '_hooks';

import type { VariantProps } from 'class-variance-authority';

const nftDisplayCardStyles = cva('flex flex-nowrap items-center', {
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
    showlabel?: boolean;
    size: NftImageProps['size'];
    borderRadius?: NftImageProps['borderRadius'];
}

export function NftThumbnail({
    objectId,
    showlabel,
    size,
    wideView,
    animateHover,
    borderRadius = 'md',
}: NFTsProps) {
    const { data: nftMeta, isLoading } = useGetNFTMeta(objectId);
    const nftTitle = nftMeta?.name || objectId;
    const truncateTitle = useMiddleEllipsis(nftTitle);

    const fileExtensionType = useFileExtensionType(nftMeta?.url || '');
    return (
        <div className={nftDisplayCardStyles({ animateHover, wideView })}>
            <Loading loading={isLoading}>
                <NftImage
                    name={nftMeta?.name || null}
                    src={nftMeta?.url || null}
                    animateHover={true}
                    showLabel={!wideView}
                    borderRadius={borderRadius}
                    size={size}
                />

                {wideView ? (
                    <div className="flex flex-col gap-1 flex-1 min-w-0">
                        <div className="capitalize text-gray-100 truncate font-semibold text-base ws-nowrap">
                            {nftMeta?.name}
                        </div>
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
                ) : showlabel ? (
                    <div
                        className={cl(
                            'flex-1 mt-2 text-steel-dark truncate overflow-hidden max-w-full',
                            animateHover &&
                                'group-hover:text-black duration-200 ease-ease-in-out-cubic'
                        )}
                    >
                        {truncateTitle}
                    </div>
                ) : null}
            </Loading>
        </div>
    );
}
