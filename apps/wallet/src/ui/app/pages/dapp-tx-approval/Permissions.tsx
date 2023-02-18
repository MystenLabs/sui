// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAddress } from '@mysten/sui.js';
import cl from 'classnames';
import { useEffect, useState } from 'react';

import { MiniNFT } from './MiniNFT';
import { SummaryCard } from './SummaryCard';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import { useGetNFTMeta } from '_hooks';

import st from './DappTxApprovalPage.module.scss';

type TabType = 'transfer' | 'modify' | 'read';

interface MetadataGroup {
    name: string;
    children: { id: string; module: string }[];
}

export function PassedObject({ id, module }: { id: string; module: string }) {
    const {data:nftMeta } = useGetNFTMeta(id);

    return (
        <div className={st.permissionsContent}>
            <div className={st.permissionsContentLabel}>
                <ExplorerLink
                    type={ExplorerLinkType.object}
                    objectID={id}
                    className={cl(st.objectId, 'text-sui-dark')}
                    showIcon={false}
                >
                    {formatAddress(id)}
                </ExplorerLink>
                <div className={st.objectName}>{module}</div>
            </div>

            {nftMeta && (
                <MiniNFT
                    url={nftMeta.url}
                    name={nftMeta?.name || 'NFT Image'}
                    size="sm"
                />
            )}
        </div>
    );
}

type PermissionsProps = {
    metadata: {
        transfer: MetadataGroup;
        modify: MetadataGroup;
        read: MetadataGroup;
    } | null;
};

export function Permissions({ metadata }: PermissionsProps) {
    const [tab, setTab] = useState<TabType | null>(null);
    // Set the initial tab state to whatever is visible:
    useEffect(() => {
        if (tab || !metadata) return;
        setTab(
            metadata.transfer.children.length
                ? 'transfer'
                : metadata.modify.children.length
                ? 'modify'
                : metadata.read.children.length
                ? 'read'
                : null
        );
    }, [tab, metadata]);

    if (!metadata || !tab) return null;

    return (
        <SummaryCard header="Permissions requested">
            <div className={st.content}>
                <div className={st.tabs}>
                    {Object.entries(metadata).map(
                        ([key, value]) =>
                            value.children.length > 0 && (
                                <button
                                    type="button"
                                    key={key}
                                    className={cl(
                                        st.tab,
                                        tab === key && st.active
                                    )}
                                    onClick={() => {
                                        setTab(key as TabType);
                                    }}
                                >
                                    {value.name}
                                </button>
                            )
                    )}
                </div>
                <div className={st.objects}>
                    {metadata[tab].children.map(({ id, module }, index) => (
                        <PassedObject key={index} id={id} module={module} />
                    ))}
                </div>
            </div>
        </SummaryCard>
    );
}
