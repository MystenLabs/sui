// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import st from './DappTxApprovalPage.module.scss';

export function MiniNFT({
    size = 'tiny',
    url,
    name,
}: {
    size?: 'tiny' | 'small';
    url: string;
    name?: string | null;
}) {
    const sizes = size === 'tiny' ? st.nftImageTiny : st.nftImageSmall;
    return <img src={url} className={sizes} alt={name || 'Nft Image'} />;
}
