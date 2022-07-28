// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import NFTdisplay from '_components/nft-display';
import ObjectsLayout from '_components/objects-layout';
import { useAppSelector } from '_hooks';
import { accountNftsSelector } from '_redux/slices/account';

import st from './NFTPage.module.scss';
function NftsPage() {
    const nfts = useAppSelector(accountNftsSelector);

    return (
        <ObjectsLayout totalItems={nfts.length} emptyMsg="No NFTs found">
            <h4 className={st.title}>
                NFT Collectibles <span>{nfts.length}</span>
            </h4>
            <section className={st.nftGalleryContainer}>
                <section className={st.nftGallery}>
                    {nfts.map((anNft, index) => (
                        <NFTdisplay nftobj={anNft} key={index} />
                    ))}
                </section>
            </section>
        </ObjectsLayout>
    );
}

export default NftsPage;
