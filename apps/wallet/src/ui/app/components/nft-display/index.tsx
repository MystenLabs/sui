// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import cl from 'classnames';

import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
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
    expandable?: boolean;
    wideview?: boolean;
};

function NFTDisplayCard({
    nftobj,
    showlabel,
    size = 'medium',
    expandable,
    wideview,
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

    const defaultSection = (
        <>
            {expandable ? (
                <div className={st.expandable}>
                    <ExplorerLink
                        type={ExplorerLinkType.object}
                        objectID={nftObjectID}
                        showIcon={false}
                        className={st['explorer-link']}
                    >
                        View Image <Icon icon={SuiIcons.Preview} />
                    </ExplorerLink>
                </div>
            ) : null}
            {showlabel && displayTitle ? (
                <div className={st.nftfields}>{displayTitle}</div>
            ) : null}
        </>
    );

    return (
        <div className={cl(st.nftimage, wideview && st.wideview, st[size])}>
            {filePath ? (
                <img
                    className={cl(st.img)}
                    src={filePath}
                    alt={fileExtensionType?.name || 'NFT'}
                    title={nftTypeShort}
                />
            ) : (
                <div className={st.noMedia} title={nftTypeShort}>
                    <Icon
                        className={st.noMediaIcon}
                        icon={SuiIcons.NftTypeImage}
                    />
                    {wideview ? null : (
                        <span className={st.noMediaText}>No media</span>
                    )}
                </div>
            )}
            {wideview ? wideviewSection : defaultSection}
        </div>
    );
}

export default NFTDisplayCard;
