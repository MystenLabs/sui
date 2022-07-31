// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiMoveObject } from '@mysten/sui.js';
import cl from 'classnames';
import { useMemo } from 'react';
import { Navigate, useSearchParams, Link } from 'react-router-dom';

import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import PageTitle from '_app/shared/page-title';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import ExternalLink from '_components/external-link';
import Icon, { SuiIcons } from '_components/icon';
import NFTDisplayCard from '_components/nft-display';
import {
    useAppSelector,
    useFileExtentionType,
    useMiddleEllipsis,
} from '_hooks';
import { accountNftsSelector } from '_redux/slices/account';

import st from './NFTDetails.module.scss';

function NFTDetialsPage() {
    const [searchParams] = useSearchParams();
    const objectId = useMemo(
        () => searchParams.get('objectId'),
        [searchParams]
    );

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

    const shortAddress = useMiddleEllipsis(nftFields?.info.id || '', 10);
    const fileExtentionType = useFileExtentionType(nftFields?.url || '');

    if (!objectId || !selectedNFT) {
        return <Navigate to="/nfts" replace={true} />;
    }

    const NFTDetails = nftFields && (
        <div className={st.nftDetails}>
            <div className={st.nftItemDetail}>
                <div className={st.label}>Object ID</div>
                <div className={st.value}>
                    <ExplorerLink
                        type={ExplorerLinkType.address}
                        address={nftFields.info.id}
                        title="View on Sui Explorer"
                        className={st.explorerLink}
                        showIcon={false}
                    >
                        {shortAddress}
                    </ExplorerLink>
                </div>
            </div>
            <div className={st.nftItemDetail}>
                <div className={st.label}>Media Type</div>
                <div className={st.value}>{fileExtentionType}</div>
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
                        <NFTDisplayCard nftobj={selectedNFT} size="large" />
                        {NFTDetails}
                    </section>
                </Content>
                <Menu stuckClass={st.shadow} className={st.shadow}>
                    <Link
                        to={`/send-nft?${new URLSearchParams({
                            objectId: selectedNFT.reference.objectId,
                        }).toString()}`}
                        className={cl(
                            'btn',
                            st.action,
                            st.sendNftBtn,
                            'neutral'
                        )}
                    >
                        <Icon
                            icon={SuiIcons.ArrowLeft}
                            className={cl(st.arrowActionIcon, st.angledArrow)}
                        />
                        Send NFT
                    </Link>
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
