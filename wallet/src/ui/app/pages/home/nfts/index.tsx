// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { hasPublicTransfer } from '@mysten/sui.js';
import { Link } from 'react-router-dom';

import { Content } from '_app/shared/bottom-menu-layout';
import PageTitle from '_app/shared/page-title';
import NFTdisplay from '_components/nft-display';
import { useAppDispatch, useAppSelector } from '_hooks';
import { accountNftsSelector } from '_redux/slices/account';
import { setNavVisibility } from '_redux/slices/app';

import st from './NFTPage.module.scss';

function NftsPage() {
    const nfts = useAppSelector(accountNftsSelector);
    //const dispatch = useAppDispatch();
    // dispatch(setNavVisibility(false));
    console.log(nfts);
    return (
        <div className={st.container}>
            <PageTitle
                title="NFT Collectibles"
                stats={`${nfts.length}`}
                className={st.pageTitle}
            />
            <Content>
                <section className={st.nftGalleryContainer}>
                    <section className={st.nftGallery}>
                        {nfts
                            .filter((nft) => hasPublicTransfer(nft))
                            .map((nft, index) => (
                                <Link
                                    to={`/send-nft?${new URLSearchParams(
                                        nft.reference.objectId
                                    ).toString()}`}
                                    key={index}
                                >
                                    <NFTdisplay nftobj={nft} />
                                </Link>
                            ))}
                    </section>
                </section>
            </Content>
        </div>
    );
}

export default NftsPage;
