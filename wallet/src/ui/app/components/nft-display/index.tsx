// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import cl from 'classnames';

import { useMediaUrl } from '_hooks';

import type { SuiObject as SuiObjectType } from '@mysten/sui.js';

import st from './NFTDisplay.module.scss';

export type SuiObjectProps = {
    nftobj: SuiObjectType;
};

function NFTdisplay({ nftobj }: SuiObjectProps) {
    const imgUrl = useMediaUrl(nftobj.data);
    return (
        <div className={cl(st.nftimage)}>
            {imgUrl ? <img className={st.img} src={imgUrl} alt="NFT" /> : null}
        </div>
    );
}

export default NFTdisplay;
