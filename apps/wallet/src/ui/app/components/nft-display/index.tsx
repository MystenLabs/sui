// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAddress, type SuiObject as SuiObjectType } from '@mysten/sui.js';
import { cva } from 'class-variance-authority';
import cl from 'classnames';

import { NftImage, type NftImageProps } from './NftImage';
import { useNFTBasicData, useOriginbyteNft } from '_hooks';

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
    nftobj: SuiObjectType;
    showlabel?: boolean;
    size: NftImageProps['size'];
    borderRadius?: NftImageProps['borderRadius'];
}

function NFTDisplayCard({
    nftobj,
    showlabel,
    size,
    wideView,
    animateHover,
    borderRadius = 'md',
}: NFTsProps) {
    const { filePath, nftObjectID, nftFields, fileExtensionType, objType } =
        useNFTBasicData(nftobj);

    const { data: originByteNft } = useOriginbyteNft(nftObjectID);

    const name = nftFields?.name || nftFields?.metadata?.fields?.name;

    const displayTitle =
        originByteNft?.fields.name ||
        (typeof name === 'string' ? name : formatAddress(nftObjectID || ''));

    return (
        <div className={nftDisplayCardStyles({ animateHover, wideView })}>
            <NftImage
                src={originByteNft?.fields.url || filePath}
                name={originByteNft?.fields.name || fileExtensionType.name}
                title={originByteNft?.fields.description || objType || ''}
                showLabel={!wideView}
                animateHover={animateHover}
                borderRadius={borderRadius}
                size={size}
            />
            {wideView ? (
                <div className="flex flex-col gap-1 flex-1 min-w-0">
                    <div className="capitalize text-gray-100 truncate font-semibold text-base ws-nowrap">
                        {displayTitle}
                    </div>
                    <div className="text-gray-75 text-body font-medium">
                        {filePath ? (
                            `${fileExtensionType.name} ${fileExtensionType.type}`
                        ) : (
                            <span className="uppercase font-normal text-bodySmall">
                                NO MEDIA
                            </span>
                        )}
                    </div>
                </div>
            ) : showlabel && displayTitle ? (
                <div
                    className={cl(
                        'flex-1 mt-2 text-steel-dark truncate overflow-hidden max-w-full',
                        animateHover &&
                            'group-hover:text-black duration-200 ease-ease-in-out-cubic'
                    )}
                >
                    {displayTitle}
                </div>
            ) : null}
        </div>
    );
}

export default NFTDisplayCard;
