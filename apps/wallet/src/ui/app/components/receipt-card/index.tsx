// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowUpRight12 } from '@mysten/icons';
import {
    getExecutionStatusType,
    getTotalGasUsed,
    getExecutionStatusError,
    SUI_TYPE_ARG,
    getTransactionSender,
    getTransactionDigest,
    getGasData,
} from '@mysten/sui.js';
import { useMemo } from 'react';

import { DateCard } from '../../shared/date-card';
import { ReceiptCardBg } from './ReceiptCardBg';
import { SponsoredTxnGasSummary } from './SponsoredTxnGasSummary';
import { StatusIcon } from './StatusIcon';
import { TxnAddressLink } from './TxnAddressLink';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import { StakeTxnCard } from '_components/receipt-card/StakeTxnCard';
import { TxnAddress } from '_components/receipt-card/TxnAddress';
import { TxnAmount } from '_components/receipt-card/TxnAmount';
import { UnStakeTxnCard } from '_components/receipt-card/UnstakeTxnCard';
import { TxnImage } from '_components/transactions-card/TxnImage';
import { notEmpty } from '_helpers';
import {
    UNSTAKE_REQUEST_EVENT_TYPE,
    STAKE_REQUEST_EVENT_TYPE,
} from '_src/shared/constants';
import { TxnGasSummary } from '_src/ui/app/components/receipt-card/TxnGasSummary';
import { Text } from '_src/ui/app/shared/text';

import type { SuiTransactionResponse, SuiAddress } from '@mysten/sui.js';

type ReceiptCardProps = {
    txn: SuiTransactionResponse;
    activeAddress: SuiAddress;
};

export function ReceiptCard({ txn, activeAddress }: ReceiptCardProps) {
    const { events, balanceChanges, objectChanges } = txn;
    const timestamp = txn.timestampMs;
    const executionStatus = getExecutionStatusType(txn);
    const error = getExecutionStatusError(txn);
    const isSuccessful = executionStatus === 'success';

    const objectId = useMemo(() => {
        const resp = objectChanges?.find((item) => {
            if (
                'owner' in item &&
                item.owner !== 'Immutable' &&
                'AddressOwner' in item.owner
            ) {
                return item.owner.AddressOwner === activeAddress;
            }
            return false;
        });
        return resp && 'objectId' in resp ? resp.objectId : null;
    }, [activeAddress, objectChanges]);

    const amountChange = useMemo(() => {
        return balanceChanges
            ?.map(({ amount, coinType, owner }) => {
                const addressOwner =
                    owner !== 'Immutable' && 'AddressOwner' in owner
                        ? owner.AddressOwner
                        : null;
                const ObjectOwner =
                    owner !== 'Immutable' && 'ObjectOwner' in owner
                        ? owner.ObjectOwner
                        : null;
                const isSender =
                    addressOwner === activeAddress ||
                    ObjectOwner === activeAddress;
                if (addressOwner !== activeAddress) {
                    return {
                        amount,
                        coinType,
                        isSender,
                        address: addressOwner ?? ObjectOwner,
                    };
                }
                return null;
            })
            .filter(notEmpty);
    }, [activeAddress, balanceChanges]);

    const totalSuiAmount =
        amountChange?.find(({ coinType }) => coinType === SUI_TYPE_ARG)
            ?.amount ?? 0;

    const { owner } = getGasData(txn)!;
    const transactionSender = getTransactionSender(txn);
    const isSender = activeAddress === transactionSender;
    const isSponsoredTransaction = transactionSender !== owner;
    const gasTotal = getTotalGasUsed(txn);

    const showGasSummary = isSuccessful && isSender && gasTotal;
    const showSponsorInfo = !isSuccessful && isSender && isSponsoredTransaction;
    const stakedTxn = events?.find(
        ({ type }) => type === STAKE_REQUEST_EVENT_TYPE
    );

    const unstakeTxn = events?.find(
        ({ type }) => type === UNSTAKE_REQUEST_EVENT_TYPE
    );

    let txnGasSummary: JSX.Element | undefined;
    if (showGasSummary && isSponsoredTransaction) {
        txnGasSummary = (
            <SponsoredTxnGasSummary sponsor={owner} totalGas={gasTotal} />
        );
    } else if (showGasSummary) {
        txnGasSummary = (
            <TxnGasSummary
                totalGas={gasTotal}
                transferAmount={+totalSuiAmount}
            />
        );
    }

    let txnStatusText = '';
    if (isSender && isSuccessful) {
        txnStatusText = 'Sent';
    } else if (isSender && !isSuccessful) {
        txnStatusText = 'Failed to Send';
    } else {
        txnStatusText = 'Received';
    }

    // const nftObjectLabel = transferAmount?.length ? txnStatusText : 'Call';

    return (
        <div className="block relative w-full">
            <div className="flex mt-2.5 justify-center items-start">
                <StatusIcon status={isSuccessful} />
            </div>
            {timestamp && (
                <div className="my-3 flex justify-center">
                    <DateCard timestamp={timestamp} size="md" />
                </div>
            )}

            <ReceiptCardBg status={executionStatus}>
                <div className="divide-y divide-solid divide-steel/20 divide-x-0 flex flex-col pt-3.5 first:pt-0">
                    {error && (
                        <div className="py-3.5 first:pt-0 break-words">
                            <Text
                                variant="body"
                                weight="medium"
                                color="issue-dark"
                            >
                                {error}
                            </Text>
                        </div>
                    )}
                    {stakedTxn ? <StakeTxnCard event={stakedTxn} /> : null}
                    {unstakeTxn ? <UnStakeTxnCard event={unstakeTxn} /> : null}
                    {!stakedTxn && !unstakeTxn ? (
                        <>
                            {objectId && (
                                <TxnImage
                                    id={objectId}
                                    actionLabel={activeAddress}
                                />
                            )}

                            {amountChange?.map(
                                ({ amount, coinType, address, isSender }) => {
                                    return (
                                        <div
                                            key={coinType + address}
                                            className="divide-y divide-solid divide-steel/20 divide-x-0 flex flex-col pt-3.5 first:pt-0"
                                        >
                                            <TxnAmount
                                                amount={amount}
                                                label={txnStatusText}
                                                coinType={coinType}
                                            />

                                            <TxnAddress
                                                address={address!}
                                                label={isSender ? 'To' : 'From'}
                                            />
                                        </div>
                                    );
                                }
                            )}
                        </>
                    ) : null}

                    {showSponsorInfo && (
                        <div className="flex justify-between items-center py-3.5">
                            <Text
                                variant="p1"
                                weight="medium"
                                color="steel-darker"
                            >
                                Sponsor
                            </Text>
                            <TxnAddressLink address={owner} />
                        </div>
                    )}

                    {txnGasSummary}

                    <div className="flex gap-1.5 w-full py-3.5">
                        <ExplorerLink
                            type={ExplorerLinkType.transaction}
                            transactionID={getTransactionDigest(txn)}
                            title="View on Sui Explorer"
                            className="text-sui-dark text-p4 font-semibold no-underline uppercase tracking-wider"
                            showIcon={false}
                        >
                            View on Explorer
                        </ExplorerLink>
                        <ArrowUpRight12 className="text-steel text-p3" />
                    </div>
                </div>
            </ReceiptCardBg>
        </div>
    );
}
