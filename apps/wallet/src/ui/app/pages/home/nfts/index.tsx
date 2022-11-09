// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Link } from 'react-router-dom';

import { Content } from '_app/shared/bottom-menu-layout';
import PageTitle from '_app/shared/page-title';
import { ErrorBoundary } from '_components/error-boundary';
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
            <Content>
                <section className={st.nftGalleryContainer}>
                    <section className={st.nftGallery}>
                        {nfts.map((nft) => (
                            <Link
                                to={`/nft-details?${new URLSearchParams({
                                    objectId: nft.reference.objectId,
                                }).toString()}`}
                                key={nft.reference.objectId}
                            >
                                <ErrorBoundary>
                                    <NFTdisplay nftobj={nft} showlabel={true} />
                                </ErrorBoundary>
                            </Link>
                        ))}
                    </section>
                </section>
            </Content>
        </div>
    );
}

export default NftsPage;
