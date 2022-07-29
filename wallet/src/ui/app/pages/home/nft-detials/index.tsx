// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiMoveObject } from '@mysten/sui.js';
import cl from 'classnames';
import { useMemo } from 'react';
import { Navigate, useSearchParams } from 'react-router-dom';

//useNavigate,
import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import PageTitle from '_app/shared/page-title';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import ExternalLink from '_components/external-link';
import Icon, { SuiIcons } from '_components/icon';
import NFTdisplay from '_components/nft-display';
import { useAppSelector } from '_hooks';
import { accountNftsSelector } from '_redux/slices/account';

import st from './NFTDetails.module.scss';

function NFTDetialsPage() {
    const [searchParams] = useSearchParams();
    const objectId = useMemo(
        () => searchParams.get('objectId'),
        [searchParams]
    );

    //, useMiddleEllipsis
    let selectedNFT;
    let nftFields;
    const nftCollections = useAppSelector(accountNftsSelector);
    if (nftCollections && nftCollections.length) {
        selectedNFT = nftCollections.filter(
            (nftItems) => nftItems.reference.objectId === objectId
        )[0];
    }

    if (selectedNFT) {
        nftFields = isSuiMoveObject(selectedNFT.data)
            ? selectedNFT.data.fields
            : null;
    }
    console.log(nftFields);

    if (!objectId || !selectedNFT) {
        return <Navigate to="/nfts" replace={true} />;
    }

    const NFTDetails = nftFields && (
        <div className={st.nftDetails}>
            <div>Object ID</div>
            <div>
                <ExplorerLink
                    type={ExplorerLinkType.address}
                    address={nftFields.info.id}
                    title="View on Sui Explorer"
                    className={st.explorerLink}
                >
                    {nftFields.info.id}
                </ExplorerLink>
            </div>
        </div>
    );

    return (
        <div className={st.container}>
            <PageTitle
                title={nftFields?.name}
                backLink="/nfts"
                className={st.pageTitle}
            />
            <BottomMenuLayout>
                <Content>
                    <section className={st.nftDetail}>
                        <NFTdisplay nftobj={selectedNFT} size="large" />
                        {NFTDetails}
                    </section>
                </Content>
                <Menu stuckClass={st.shadow} className={st.shadow}>
                    <Button size="large" mode="neutral" className={st.action}>
                        <Icon
                            icon={SuiIcons.ArrowRight}
                            className={cl(st.arrowActionIcon, st.angledArrow)}
                        />
                        Send NFT
                    </Button>
                    {nftFields?.url && (
                        <ExternalLink
                            href={nftFields.url}
                            showIcon={false}
                            className={cl(st.action, st.externalLink)}
                        >
                            <Icon
                                icon={SuiIcons.Nfts}
                                className={st.arrowActionIcon}
                            />
                            View Image
                        </ExternalLink>
                    )}
                </Menu>
            </BottomMenuLayout>
        </div>
    );
}

export default NFTDetialsPage;
