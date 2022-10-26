// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Link } from 'react-router-dom';

import PageTitle from '_app/shared/page-title';
import NFTdisplay from '_components/nft-display';
import { useAppSelector } from '_hooks';
import { accountNftsSelector } from '_redux/slices/account';

import st from './NFTPage.module.scss';

function NftsPage() {
    const nfts = useAppSelector(accountNftsSelector);

    return (
        <div className={st.container}>
            <PageTitle
                title="NFTs"
                stats={`${nfts.length}`}
                className={st.pageTitle}
            />
            <div className={st.nftGallery}>
                {nfts.map((nft) => (
                    <Link
                        to={`/nft-details?${new URLSearchParams({
                            objectId: nft.reference.objectId,
                        }).toString()}`}
                        key={nft.reference.objectId}
                        className={st.galleryItem}
                    >
                        <NFTdisplay nftobj={nft} showlabel={true} />
                    </Link>
                ))}
            </div>
        </div>
    );
}

export default NftsPage;
