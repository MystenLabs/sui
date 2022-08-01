// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { isSuiMoveObject } from '@mysten/sui.js';
import cl from 'classnames';

import { useFileExtentionType, useMediaUrl } from '_hooks';

import type { SuiObject as SuiObjectType } from '@mysten/sui.js';

import st from './NFTDisplay.module.scss';

export type NFTsProps = {
    nftobj: SuiObjectType;
    showlabel?: boolean;
    size?: 'small' | 'medium' | 'large';
    expandable?: boolean;
    showNTFType?: boolean;
};

function NFTDisplayCard({
    nftobj,
    showlabel,
    size = 'medium',
    expandable,
}: NFTsProps) {
    const imgUrl = useMediaUrl(nftobj.data);
    const nftFields = isSuiMoveObject(nftobj.data) ? nftobj.data.fields : null;
    const fileExtentionType = useFileExtentionType(nftFields?.url || '');

    return (
        <div className={cl(st.nftimage, st.showNTFType)}>
            {imgUrl ? (
                <img className={cl(st.img, st[size])} src={imgUrl} alt="NFT" />
            ) : null}
            {expandable && <div className={st.expandable}>View Image</div>}
            {showlabel && nftFields?.name && (
                <div className={st.nftfields}>{nftFields.name}</div>
            )}
        </div>
    );
}

export default NFTDisplayCard;
