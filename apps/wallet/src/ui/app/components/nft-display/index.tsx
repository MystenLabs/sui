// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';

import Icon, { SuiIcons } from '_components/icon';
import { useMiddleEllipsis, useNFTBasicData } from '_hooks';

import type { SuiObject as SuiObjectType } from '@mysten/sui.js';

import st from './NFTDisplay.module.scss';

const OBJ_TYPE_MAX_LENGTH = 20;
const OBJ_TYPE_MAX_PREFIX_LENGTH = 3;

export type NFTsProps = {
    nftobj: SuiObjectType;
    showlabel?: boolean;
    size?: 'small' | 'medium' | 'large';
    wideview?: boolean;
    animateHover?: boolean;
    borderRadius?: 'md' | 'lg';
};

function NFTDisplayCard({
    nftobj,
    showlabel,
    size = 'medium',
    wideview,
    animateHover,
    borderRadius = 'md',
}: NFTsProps) {
    const { filePath, nftObjectID, nftFields, fileExtensionType, objType } =
        useNFTBasicData(nftobj);

    const name = nftFields?.name || nftFields?.metadata?.fields?.name;
    const objIDShort = useMiddleEllipsis(nftObjectID);
    const nftTypeShort = useMiddleEllipsis(
        objType,
        OBJ_TYPE_MAX_LENGTH,
        OBJ_TYPE_MAX_PREFIX_LENGTH
    );
    const displayTitle = name || objIDShort;
    const wideviewSection = (
        <div className={st.nftfields}>
            <div className={st.nftName}>{displayTitle}</div>
            <div className={st.nftType}>
                {filePath ? (
                    `${fileExtensionType.name} ${fileExtensionType.type}`
                ) : (
                    <span className={st.noMediaTextWideView}>NO MEDIA</span>
                )}
            </div>
        </div>
    );

    const defaultSection =
        showlabel && displayTitle ? (
            <div
                className={cl(
                    'items-center mt-2 text-sui-steel-dark',
                    animateHover &&
                        'group-hover:text-black duration-200 ease-ease-in-out-cubic'
                )}
            >
                {displayTitle}
            </div>
        ) : null;

    const borderRadiusCl = borderRadius === 'md' ? 'rounded' : 'rounded-[10px]';
    const borderRadiusHoverCl =
        borderRadius === 'md' ? 'rounded-sm' : 'rounded-[5px]';
    const mediaContainerCls = animateHover
        ? `ease-ease-out-cubic duration-[400ms] group-hover:shadow-sui-steel/50 group-hover:shadow-[0_0_20px_0] ${borderRadiusCl} hover:${borderRadiusHoverCl}`
        : '';
    const mediaCls = animateHover
        ? 'group-hover:scale-[115%] duration-500 ease-ease-out-cubic'
        : '';
    return (
        <div
            className={cl(
                st.nftimage,
                wideview && st.wideview,
                st[size],
                'group'
            )}
        >
            <div
                className={cl(
                    'flex flex-shrink-0 items-stretch flex-grow self-stretch overflow-hidden',
                    mediaContainerCls
                )}
            >
                {filePath ? (
                    <img
                        className={cl(st.img, 'rounded-none', mediaCls)}
                        src={filePath}
                        alt={fileExtensionType?.name || 'NFT'}
                        title={nftTypeShort}
                    />
                ) : (
                    <div
                        className={cl(st.noMedia, 'rounded-none', mediaCls)}
                        title={nftTypeShort}
                    >
                        <Icon
                            className={st.noMediaIcon}
                            icon={SuiIcons.NftTypeImage}
                        />
                        {wideview ? null : (
                            <span className={st.noMediaText}>No media</span>
                        )}
                    </div>
                )}
            </div>
            {wideview ? wideviewSection : defaultSection}
        </div>
    );
}

export default NFTDisplayCard;
