// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getPaySuiTransaction,
    getPayTransaction,
    getTransferSuiTransaction,
    getTransferObjectTransaction,
    getTransactionKindName,
    getTransactionSender,
    getTransactions,
    SUI_TYPE_ARG,
    CoinBalanceChangeEvent,
    is,
} from '@mysten/sui.js';

import type {
    SuiTransactionKind,
    TransactionEffects,
    SuiTransactionResponse,
 
} from '@mysten/sui.js';

const getCoinType = (
    txEffects: TransactionEffects | null,
    address: string
): string | null => {
    if (!txEffects) return null;
    
    const events = txEffects?.events || [];
    const coinEvent = events.find((event) => {
        if (
            'coinBalanceChange' in event &&
            is(event.coinBalanceChange, CoinBalanceChangeEvent) &&
            ['Receive', 'Pay'].includes(event.coinBalanceChange.changeType) &&
            event.coinBalanceChange.transactionModule !== 'gas'
        ) {
            const { owner, sender } = event.coinBalanceChange;
            const { AddressOwner } = owner as { AddressOwner: string };
            return AddressOwner === address || address === sender;
        }
        return false;
    }) as { coinBalanceChange: CoinBalanceChangeEvent } | undefined;

    return coinEvent?.coinBalanceChange.coinType || null;
};

type FormattedBalance = {
    amount?: number | null;
    coinType?: string | null;
    address: string;
};

// For TransferObject, TransferSui, Pay, PaySui, transactions get the amount from the transfer data
export function getTransfersAmount(
    txnData: SuiTransactionKind,
    txnEffect?: TransactionEffects
): FormattedBalance[] | null {
    const txKindName = getTransactionKindName(txnData);
    if (txKindName === 'TransferObject') {
        const txn = getTransferObjectTransaction(txnData);
        return txn?.recipient
            ? [
                  {
                      address: txn?.recipient,
                  },
              ]
            : null;
    }

    if (txKindName === 'TransferSui') {
        const txn = getTransferSuiTransaction(txnData);
        return txn?.recipient
            ? [
                  {
                      address: txn.recipient,
                      amount: txn?.amount,
                      coinType:
                          txnEffect && getCoinType(txnEffect, txn.recipient),
                  },
              ]
            : null;
    }

    const payData = getPaySuiTransaction(txnData) ?? getPayTransaction(txnData);

    const amountByRecipient = payData?.recipients.reduce(
        (acc, recipient, index) => ({
            ...acc,
            [recipient]: {
                amount:
                    payData.amounts[index] +
                    (recipient in acc ? acc[recipient].amount : 0),

                // for PaySuiTransaction the coinType is SUI
                coinType:
                    txKindName === 'PaySui'
                        ? SUI_TYPE_ARG
                        : getCoinType(txnEffect || null, recipient),
                address: recipient,
            },
        }),
        {} as {
            [key: string]: {
                amount: number;
                coinType: string | null;
                address: string;
            };
        }
    );
    return amountByRecipient ? Object.values(amountByRecipient) : null;
}

// Get transaction amount from coinBalanceChange event for Call Txn
// Aggregate coinBalanceChange by coinType and address
function getTxnAmountFromCoinBalanceEvent(
    txEffects: TransactionEffects,
    address: string
): FormattedBalance[] {
    const events = txEffects?.events || [];
    const coinsMeta = {} as { [coinType: string]: FormattedBalance };

    events.forEach((event) => {
        if (
            'coinBalanceChange' in event &&
            event?.coinBalanceChange?.changeType &&
            ['Receive', 'Pay'].includes(event?.coinBalanceChange?.changeType) &&
            event?.coinBalanceChange?.transactionModule !== 'gas'
        ) {
            const { coinBalanceChange } = event;
            const { coinType, amount, owner, sender } = coinBalanceChange;
            const { AddressOwner } = owner as { AddressOwner: string };
            if (AddressOwner === address || address === sender) {
                coinsMeta[`${AddressOwner}${coinType}`] = {
                    amount:
                        (coinsMeta[`${AddressOwner}${coinType}`]?.amount || 0) +
                        amount,
                    coinType: coinType,
                    address: AddressOwner,
                };
            }
        }
    });
    return Object.values(coinsMeta);
}

// Get the amount from events and transfer data
// optional flag to get only SUI coin type for table view
export function getAmount({
    txnData,
    suiCoinOnly = false,
}: {
    txnData: SuiTransactionResponse;
    suiCoinOnly?: boolean;
}) {
    const { effects, certificate } = txnData;
    const txnDetails = getTransactions(certificate)[0];
    const sender = getTransactionSender(certificate);
    const suiTransfer = getTransfersAmount(txnDetails, effects);
    const coinBalanceChange = getTxnAmountFromCoinBalanceEvent(effects, sender);
    const transfers = suiTransfer || coinBalanceChange;
    if (suiCoinOnly) {
        return transfers?.filter(({ coinType }) => coinType === SUI_TYPE_ARG);
    }

    return transfers;
}
