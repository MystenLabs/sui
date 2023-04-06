// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetTransferLabel, useGetTransferAmount } from '@mysten/core';
import {
    getExecutionStatusType,
    getTransactionDigest,
    getTransactionSender,
    getExecutionStatusError,
    Coin,
} from '@mysten/sui.js';
import { useMemo } from 'react';
import { Link } from 'react-router-dom';

import { TxnTypeLabel } from './TxnActionLabel';
import { TxnIcon } from './TxnIcon';
import { CoinBalance } from '_app/shared/coin-balance';
import { DateCard } from '_app/shared/date-card';
import { Text } from '_app/shared/text';
import { useGetTxnRecipientAddress } from '_hooks';

import type {
    SuiAddress,
    // SuiEvent,
    SuiTransactionBlockResponse,
    // TransactionEvents,
} from '@mysten/sui.js';

// export const getTxnEffectsEventID = (
//     events: TransactionEvents,
//     address: string
// ): string[] => {
//     return events
//         ?.map((event: SuiEvent) => {
//             const data = Object.values(event).find(
//                 (itm) => itm?.recipient?.AddressOwner === address
//             );
//             return data?.objectId;
//         })
//         .filter(notEmpty);
// };

export function TransactionCard({
    txn,
    address,
}: {
    txn: SuiTransactionBlockResponse;
    address: SuiAddress;
}) {
    const executionStatus = getExecutionStatusType(txn);

    // const objectId = useMemo(() => {
    //     return getTxnEffectsEventID(txn.events!, address)[0];
    // }, [address, txn.events]);

    const { amount, coinType } = useGetTransferAmount(txn, address);

    const symbol = useMemo(() => Coin.getCoinSymbol(coinType), [coinType]);

    const recipientAddress = useGetTxnRecipientAddress({ txn, address });

    const isSender = address === getTransactionSender(txn);

    const error = getExecutionStatusError(txn);

    // Transition label - depending on the transaction type and amount
    // Epoch change without amount is delegation object
    // Special case for staking and unstaking move call transaction,
    // For other transaction show Sent or Received
    const txnLabel = useGetTransferLabel(txn, address);
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
                        // TODO: Support programmable transactions variable icons here:
                        variant="Send"
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
                            <CoinBalance amount={amount} coinType={coinType} />
                        </div>
                    ) : (
                        <>
                            <div className="flex w-full justify-between">
                                <div className="flex gap-1 align-middle items-baseline">
                                    <Text color="gray-90" weight="semibold">
                                        {txnLabel}
                                    </Text>

                                    <Text
                                        color="gray-90"
                                        weight="normal"
                                        variant="subtitleSmall"
                                    >
                                        {symbol}
                                    </Text>
                                </div>
                                <CoinBalance
                                    amount={amount}
                                    coinType={coinType}
                                />
                            </div>

                            {/* TODO: Support programmable tx: */}
                            <TxnTypeLabel
                                address={recipientAddress!}
                                isSender={isSender}
                                isTransfer={false}
                            />
                            {/* {objectId && <TxnImage id={objectId} />} */}
                        </>
                    )}

                    {timestamp && (
                        <DateCard timestamp={Number(timestamp)} size="sm" />
                    )}
                </div>
            </div>
        </Link>
    );
}
