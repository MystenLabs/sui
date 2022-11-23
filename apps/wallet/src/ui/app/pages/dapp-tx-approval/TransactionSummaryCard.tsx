// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { MiniNFT } from './MiniNFT';
import {
    SummaryCard,
    SummaryCardHeader,
    SummaryCardContent,
} from './SummaryCard';
import AccountAddress from '_components/account-address';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import ExternalLink from '_components/external-link';
import { useMiddleEllipsis, useFormatCoin, useGetNFTMeta } from '_hooks';
import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';

import type { CoinsMetaProps } from '_redux/slices/transaction-requests';

import st from './DappTxApprovalPage.module.scss';

const TRUNCATE_MAX_LENGTH = 10;
const TRUNCATE_PREFIX_LENGTH = 6;

type TransferSummerCardProps = {
    coinsMeta: CoinsMetaProps[];
    origin: string;
    objectIDs: string[];
    gasEstimate: number | null;
};

function MiniNFTLink({ id }: { id: string }) {
    const objectId = useMiddleEllipsis(
        id,
        TRUNCATE_MAX_LENGTH,
        TRUNCATE_PREFIX_LENGTH
    );
    const nftMeta = useGetNFTMeta(id);
    return (
        <>
            {nftMeta && (
                <MiniNFT
                    url={nftMeta.url}
                    name={nftMeta?.name || 'NFT Image'}
                    size="xs"
                />
            )}
            <ExplorerLink
                type={ExplorerLinkType.object}
                objectID={id}
                className={st.objectId}
                showIcon={false}
            >
                {objectId}
            </ExplorerLink>
        </>
    );
}

function CoinMeta({
    receiverAddress,
    coinMeta,
    origin,
}: {
    receiverAddress: string;
    coinMeta: CoinsMetaProps;
    origin: string;
}) {
    const [formatedAmount, symbol] = useFormatCoin(
        coinMeta.amount ? Math.abs(coinMeta.amount) : 0,
        coinMeta.coinType
    );
    return (
        <div className={st.content} key={receiverAddress}>
            <div className={st.row}>
                <div className={st.label}>Send</div>
                <div className={st.value}>
                    {formatedAmount} {symbol}
                </div>
            </div>

            <div className={st.row}>
                <div className={st.label}>To</div>
                <div className={st.value}>
                    <div className={st.value}>
                        <ExternalLink
                            href={origin}
                            className={st.origin}
                            showIcon={false}
                        >
                            {new URL(origin || '').host}
                        </ExternalLink>
                    </div>
                </div>
            </div>
        </div>
    );
}

export function TransactionSummaryCard({
    objectIDs,
    coinsMeta,
    gasEstimate,
    origin,
}: TransferSummerCardProps) {
    const [gasEst, gasSymbol] = useFormatCoin(gasEstimate || 0, GAS_TYPE_ARG);

    return (
        <SummaryCard>
            <SummaryCardHeader>Transaction summary</SummaryCardHeader>
            <SummaryCardContent>
                {coinsMeta &&
                    coinsMeta.map((coinMeta, index) => (
                        <CoinMeta
                            receiverAddress={coinMeta.receiverAddress}
                            key={index}
                            coinMeta={coinMeta}
                            origin={origin}
                        />
                    ))}
                {objectIDs.length > 0 && (
                    <div className={st.content}>
                        {objectIDs.map((objectId) => (
                            <div className={st.row} key={objectId}>
                                <div className={st.label}>Send</div>
                                <div className={st.value}>
                                    <MiniNFTLink id={objectId} />
                                </div>
                            </div>
                        ))}

                        <div className={st.row}>
                            <div className={st.label}>To</div>
                            <div className={st.value}>
                                <AccountAddress
                                    showLink={false}
                                    copyable={false}
                                    className={st.ownerAddress}
                                    mode="normal"
                                />
                            </div>
                        </div>
                    </div>
                )}

                <div className={st.cardFooter}>
                    <div>Estimated Gas Fees</div>
                    {gasEst} {gasSymbol}
                </div>
            </SummaryCardContent>
        </SummaryCard>
    );
}
