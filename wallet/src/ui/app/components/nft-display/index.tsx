// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { isSuiMoveObject } from '@mysten/sui.js';
import cl from 'classnames';

import { useMediaUrl } from '_hooks';

import type { SuiObject as SuiObjectType } from '@mysten/sui.js';

import st from './NFTDisplay.module.scss';

export type SuiObjectProps = {
    nftobj: SuiObjectType;
    showlabel?: boolean;
    size?: 'small' | 'medium' | 'large';
};

function NFTdisplay({ nftobj, showlabel, size = 'medium' }: SuiObjectProps) {
    const imgUrl = useMediaUrl(nftobj.data);
    const nftFields = isSuiMoveObject(nftobj.data) ? nftobj.data.fields : null;

    return (
        <div className={cl(st.nftimage)}>
            {imgUrl ? (
                <img className={cl(st.img, st[size])} src={imgUrl} alt="NFT" />
            ) : null}
            {showlabel && nftFields?.name && (
                <div className={st.nftfields}>{nftFields.name}</div>
            )}
        </div>
    );
}

export default NFTdisplay;
