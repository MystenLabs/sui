// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    hasPublicTransfer,
    formatAddress,
    SuiObject,
    is,
    getObjectOwner,
} from '@mysten/sui.js';
import cl from 'classnames';
import { useMemo } from 'react';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';

import Button from '_app/shared/button';
import { Collapse } from '_app/shared/collapse';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import { NFTDisplayCard } from '_components/nft-display';
import { useAppSelector, useNFTBasicData, useGetObject } from '_hooks';
import ExternalLink from '_src/ui/app/components/external-link';
import PageTitle from '_src/ui/app/shared/PageTitle';

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
                    <div className="flex-1 text-gray-80 truncate">{label}</div>
                    <div className="max-w-[60%] text-gray-90 truncate">
                        {typeof value === 'string' &&
                        (value.startsWith('http://') ||
                            value.startsWith('https://')) ? (
                            <ExternalLink
                                href={value}
                                className="text-steel-darker no-underline"
                            >
                                {value}
                            </ExternalLink>
                        ) : (
                            value
                        )}
                    </div>
                </div>
            ))}
        </div>
    );
}

const FILTER_PROPERTIES = ['id', 'url', 'name'];

function NFTDetailsPage() {
    const [searchParams] = useSearchParams();
    const navigate = useNavigate();
    const nftId = searchParams.get('objectId');
    const accountAddress = useAppSelector(({ account }) => account.address);

    const { data: objectData, isLoading } = useGetObject(nftId!);
    const selectedNft = useMemo(() => {
        if (!is(objectData?.details, SuiObject) || !objectData) return null;
        const owner = getObjectOwner(objectData) as { AddressOwner: string };
        return owner.AddressOwner === accountAddress
            ? objectData.details
            : null;
    }, [accountAddress, objectData]);

    const isTransferable = !!selectedNft && hasPublicTransfer(selectedNft);
    const { nftFields, fileExtensionType, filePath } =
        useNFTBasicData(selectedNft);

    // Extract either the attributes, or use the top-level NFT fields:
    const metaFields =
        nftFields?.metadata?.fields?.attributes?.fields ||
        Object.entries(nftFields ?? {})
            .filter(([key]) => !FILTER_PROPERTIES.includes(key))
            .reduce(
                (acc, [key, value]) => {
                    acc.keys.push(key);
                    acc.values.push(value);
                    return acc;
                },
                { keys: [] as string[], values: [] as string[] }
            );

    const metaKeys: string[] = metaFields ? metaFields.keys : [];
    const metaValues = metaFields ? metaFields.values : [];
    return (
        <div
            className={cl('flex flex-col flex-nowrap flex-1 gap-5', {
                'items-center': isLoading,
            })}
        >
            <Loading loading={isLoading}>
                {selectedNft ? (
                    <>
                        <PageTitle back="/nfts" />
                        <div className="flex flex-col flex-nowrap flex-1 items-stretch overflow-y-auto overflow-x-hidden gap-7">
                            <div className="self-center gap-3 flex flex-col flex-nowrap items-center">
                                <NFTDisplayCard objectId={nftId!} size="lg" />
                                {nftId ? (
                                    <ExplorerLink
                                        type={ExplorerLinkType.object}
                                        objectID={nftId}
                                        className={cl(
                                            'text-steel-dark no-underline flex flex-nowrap gap-2 items-center',
                                            'text-captionSmall font-semibold uppercase hover:text-hero duration-100',
                                            'ease-ease-in-out-cubic'
                                        )}
                                        showIcon={false}
                                    >
                                        VIEW ON EXPLORER{' '}
                                        <Icon
                                            icon={SuiIcons.ArrowLeft}
                                            className="rotate-135 text-subtitleSmallExtra"
                                        />
                                    </ExplorerLink>
                                ) : null}
                            </div>
                            <div className="flex-1">
                                <Collapse title="Details" initialIsOpen>
                                    <LabelValueItems
                                        items={[
                                            {
                                                label: 'Object Id',
                                                value: nftId ? (
                                                    <ExplorerLink
                                                        type={
                                                            ExplorerLinkType.object
                                                        }
                                                        objectID={nftId}
                                                        title="View on Sui Explorer"
                                                        className="text-sui-dark no-underline font-mono"
                                                        showIcon={false}
                                                    >
                                                        {formatAddress(nftId)}
                                                    </ExplorerLink>
                                                ) : null,
                                            },
                                            {
                                                label: 'Media Type',
                                                value:
                                                    filePath &&
                                                    fileExtensionType.name &&
                                                    fileExtensionType.type
                                                        ? `${fileExtensionType.name} ${fileExtensionType.type}`
                                                        : '-',
                                            },
                                        ]}
                                    />
                                </Collapse>
                            </div>
                            {metaKeys.length ? (
                                <div className="flex-1">
                                    <Collapse title="Attributes" initialIsOpen>
                                        <LabelValueItems
                                            items={metaKeys.map(
                                                (aKey, idx) => ({
                                                    label: aKey,
                                                    value:
                                                        typeof metaValues[
                                                            idx
                                                        ] === 'object'
                                                            ? JSON.stringify(
                                                                  metaValues[
                                                                      idx
                                                                  ]
                                                              )
                                                            : metaValues[idx],
                                                    key: aKey,
                                                })
                                            )}
                                        />
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
