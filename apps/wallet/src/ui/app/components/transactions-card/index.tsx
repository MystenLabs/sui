// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getExecutionStatusError,
    getExecutionStatusType,
    getTransactionDigest,
    getTransactionSender,
    SUI_TYPE_ARG,
} from '@mysten/sui.js';
import { useMemo } from 'react';
import { Link } from 'react-router-dom';

import { TxnTypeLabel } from './TxnActionLabel';
import { TxnIcon } from './TxnIcon';
import { TxnImage } from './TxnImage';
import { CoinBalance } from '_app/shared/coin-balance';
import { DateCard } from '_app/shared/date-card';
import { Text } from '_app/shared/text';

import type { SuiAddress, SuiTransactionResponse } from '@mysten/sui.js';

export function TransactionCard({
    txn,
    address,
}: {
    txn: SuiTransactionResponse;
    address: SuiAddress;
}) {
    const executionStatus = getExecutionStatusType(txn);
    const { events, objectChanges, balanceChanges } = txn;
    const objectId = useMemo(() => {
        const resp = objectChanges?.find((item) => {
            if ('owner' in item) {
                return item.owner === address;
            }
            return false;
        });
        return resp && 'objectId' in resp ? resp.objectId : null;
    }, [address, objectChanges]);

    // we only show Sui Transfer amount or the first non-Sui transfer amount
    const transferAmount = useMemo(() => {
        // Find SUI transfer amount
        const amountTransfersSui = balanceChanges?.find(
            ({ coinType }) => coinType === SUI_TYPE_ARG
        );

        // Find non-SUI transfer amount
        const amountTransfersNonSui = balanceChanges?.find(
            ({ coinType }) => coinType !== SUI_TYPE_ARG
        );

        return {
            amount:
                amountTransfersSui?.amount ||
                amountTransfersNonSui?.amount ||
                null,
            coinType:
                amountTransfersSui?.coinType ||
                amountTransfersNonSui?.coinType ||
                null,
        };
    }, [balanceChanges]);

    const recipientAddress = useMemo(() => {
        if (balanceChanges) {
            const resp = balanceChanges.find(
                ({ owner }) =>
                    owner !== 'Immutable' &&
                    'AddressOwner' in owner &&
                    owner.AddressOwner !== address
            );
            return resp &&
                resp.owner !== 'Immutable' &&
                'AddressOwner' in resp.owner
                ? resp.owner.AddressOwner
                : null;
        }
        // TODO: handle Object transfer
        return null;
    }, [balanceChanges, address]);

    const isSender = address === getTransactionSender(txn);

    const error = getExecutionStatusError(txn);

    const stakedTxn = events?.some(
        ({ type }) => type === '0x2::validator::StakingRequestEvent'
    )
        ? 'Staked'
        : null;

    const unstakeTxn = events?.some(
        ({ type }) => type === '0x2::validator::UnstakingRequestEvent'
    )
        ? 'Unstaked'
        : null;

    const sentRecieveLabel = isSender ? 'Sent' : 'Received';

    const txnLabel = stakedTxn ?? unstakeTxn ?? sentRecieveLabel;

    const showSuiSymbol =
        unstakeTxn || stakedTxn || transferAmount.coinType === SUI_TYPE_ARG;

    const transferAmountComponent = transferAmount.coinType &&
        transferAmount.amount && (
            <CoinBalance
                amount={Math.abs(+transferAmount.amount)}
                coinType={transferAmount.coinType}
            />
        );

    const timestamp = txn.timestampMs;

    return (
        <Link
            to={`/receipt?${new URLSearchParams({
                txdigest: getTransactionDigest(txn),
            }).toString()}`}
            className="flex items-center w-full flex-col gap-2 py-4 no-underline"
        >
            <div className="flex items-start w-full justify-between gap-3">
                <div className="w-7.5">
                    <TxnIcon
                        txnFailed={executionStatus !== 'success' || !!error}
                        variant={txnLabel}
                    />
                </div>
                <div className="flex flex-col w-full gap-1.5">
                    {error ? (
                        <div className="flex w-full justify-between">
                            <div className="flex flex-col w-full gap-1.5">
                                <Text color="gray-90" weight="medium">
                                    Transaction Failed
                                </Text>

                                <div className="flex break-all">
                                    <Text
                                        variant="p3"
                                        weight="normal"
                                        color="issue-dark"
                                    >
                                        {error}
                                    </Text>
                                </div>
                            </div>
                            {transferAmountComponent}
                        </div>
                    ) : (
                        <>
                            <div className="flex w-full justify-between">
                                <div className="flex gap-1 align-middle items-baseline">
                                    <Text color="gray-90" weight="semibold">
                                        {txnLabel}
                                    </Text>
                                    {showSuiSymbol && (
                                        <Text
                                            color="gray-90"
                                            weight="normal"
                                            variant="subtitleSmall"
                                        >
                                            SUI
                                        </Text>
                                    )}
                                </div>
                                {transferAmountComponent}
                            </div>

                            {/* TODO: Support programmable tx: */}
                            <TxnTypeLabel
                                address={recipientAddress!}
                                isSender={isSender}
                                isTransfer={!!recipientAddress}
                            />
                            {objectId && <TxnImage id={objectId} />}
                        </>
                    )}

                    {timestamp && <DateCard timestamp={timestamp} size="sm" />}
                </div>
            </div>
        </Link>
    );
}
