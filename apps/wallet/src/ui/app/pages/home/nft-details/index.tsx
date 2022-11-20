// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { hasPublicTransfer } from '@mysten/sui.js';
import cl from 'classnames';
import { useMemo } from 'react';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';

import Button from '_app/shared/button';
import { Collapse } from '_app/shared/collapse';
import PageTitle from '_app/shared/page-title';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import NFTDisplayCard from '_components/nft-display';
import {
    useAppSelector,
    useMiddleEllipsis,
    useNFTBasicData,
    useObjectsState,
} from '_hooks';
import { createAccountNftByIdSelector } from '_redux/slices/account';

import type { ReactNode } from 'react';

function LabelValueItems({
    items,
}: {
    items: { label: string; value: ReactNode; key?: string }[];
}) {
    return (
        <div className="flex flex-col flex-nowrap gap-3 text-body font-medium">
            {items.map(({ label, value, key }) => (
                <div
                    className="flex flex-row flex-nowrap gap-1"
                    key={key || label}
                >
                    <div className="flex-1 text-sui-grey-80 truncate">
                        {label}
                    </div>
                    <div className="max-w-[60%] text-sui-grey-90 truncate">
                        {value}
                    </div>
                </div>
            ))}
        </div>
    );
}

function NFTDetailsPage() {
    const [searchParams] = useSearchParams();
    const navigate = useNavigate();
    const nftId = searchParams.get('objectId');
    const nftSelector = useMemo(
        () => createAccountNftByIdSelector(nftId || ''),
        [nftId]
    );
    const selectedNft = useAppSelector(nftSelector);
    const isTransferable = !!selectedNft && hasPublicTransfer(selectedNft);
    const shortAddress = useMiddleEllipsis(nftId);
    const { nftFields, fileExtensionType, filePath } =
        useNFTBasicData(selectedNft);
    const { loading } = useObjectsState();
    const detailAttrs = [
        {
            label: 'Object Id',
            value: nftId ? (
                <ExplorerLink
                    type={ExplorerLinkType.object}
                    objectID={nftId}
                    title="View on Sui Explorer"
                    className="text-sui-dark no-underline font-mono"
                    showIcon={false}
                >
                    {shortAddress}
                </ExplorerLink>
            ) : null,
        },
        {
            label: 'Media Type',
            value:
                filePath && fileExtensionType.name && fileExtensionType.type
                    ? `${fileExtensionType.name} ${fileExtensionType.type}`
                    : '-',
        },
    ];
    const metaFields = nftFields?.metadata?.fields?.attributes?.fields || null;
    const metaKeys: string[] = metaFields ? metaFields.keys : [];
    const metaValues = metaFields ? metaFields.values : [];
    const metaAttrs = metaKeys.map((aKey, idx) => ({
        label: aKey,
        value:
            typeof metaValues[idx] === 'object'
                ? JSON.stringify(metaValues[idx])
                : metaValues[idx],
        key: `nft_attribute_${aKey}`,
    }));
    return (
        <div
            className={cl('flex flex-col flex-nowrap flex-1 gap-5', {
                'items-center': loading,
            })}
        >
            <Loading loading={loading}>
                {selectedNft ? (
                    <>
                        <div className="flex">
                            <PageTitle backLink="/nfts" hideBackLabel={true} />
                        </div>
                        <div className="flex flex-col flex-nowrap flex-1 items-stretch overflow-y-auto overflow-x-hidden gap-[30px]">
                            <div className="self-center gap-3 flex flex-col flex-nowrap items-center">
                                <NFTDisplayCard
                                    nftobj={selectedNft}
                                    size="large"
                                />
                                {nftId ? (
                                    <ExplorerLink
                                        type={ExplorerLinkType.object}
                                        objectID={nftId}
                                        className={cl(
                                            'text-sui-steel-dark no-underline flex flex-nowrap gap-2 items-center',
                                            'text-captionSmall font-semibold uppercase hover:text-hero duration-100',
                                            'ease-ease-in-out-cubic'
                                        )}
                                        showIcon={false}
                                    >
                                        VIEW ON EXPLORER{' '}
                                        <Icon
                                            icon={SuiIcons.ArrowLeft}
                                            className="rotate-[135deg] text-[10px]"
                                        />
                                    </ExplorerLink>
                                ) : null}
                            </div>
                            <div className="flex-1">
                                <Collapse title="Details">
                                    <LabelValueItems items={detailAttrs} />
                                </Collapse>
                            </div>
                            {metaAttrs.length ? (
                                <div className="flex-1">
                                    <Collapse title="Attributes">
                                        <LabelValueItems items={metaAttrs} />
                                    </Collapse>
                                </div>
                            ) : null}
                            <div className="flex-1 flex items-end mb-3">
                                <Button
                                    mode="primary"
                                    className="flex-1"
                                    disabled={!isTransferable}
                                    title={
                                        isTransferable
                                            ? undefined
                                            : "Unable to send. NFT doesn't have public transfer method"
                                    }
                                    onClick={() => {
                                        navigate(`/nft-transfer/${nftId}`);
                                    }}
                                >
                                    Send NFT
                                    <Icon
                                        icon={SuiIcons.ArrowLeft}
                                        className="rotate-180 text-xs"
                                    />
                                </Button>
                            </div>
                        </div>
                    </>
                ) : (
                    <Navigate to="/nfts" replace={true} />
                )}
            </Loading>
        </div>
    );
}

export default NFTDetailsPage;
