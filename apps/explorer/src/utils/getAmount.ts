// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getPaySuiTransaction,
    getPayTransaction,
    getTransferSuiTransaction,
    getTransferObjectTransaction,
    getTransactionKindName,
} from '@mysten/sui.js';

import type {
    SuiTransactionKind,
    TransactionEffects,
    SuiEvent,
} from '@mysten/sui.js';

const getCoinType = (
    txEffects: TransactionEffects,
    address: string
): string | null => {
    const events = txEffects?.events || [];
    const coinType = events
        ?.map((event: SuiEvent) => {
            const data = Object.values(event).find(
                (itm) => itm?.owner?.AddressOwner === address
            );
            return data?.coinType;
        })
        .filter(Boolean);
    return coinType?.[0] ? coinType[0] : null;
};

type FormattedBalance = {
    amount?: number | bigint | null;
    coinType?: string | null;
    isCoin?: boolean;
    recipientAddress: string;
}[];

export function getAmount(
    txnData: SuiTransactionKind,
    txnEffect?: TransactionEffects
): FormattedBalance | null {
    const txKindName = getTransactionKindName(txnData);
    if (txKindName === 'TransferObject') {
        const txn = getTransferObjectTransaction(txnData);
        return txn?.recipient
            ? [
                  {
                      recipientAddress: txn?.recipient,
                  },
              ]
            : null;
    }

    if (txKindName === 'TransferSui') {
        const txn = getTransferSuiTransaction(txnData);
        return txn?.recipient
            ? [
                  {
                      recipientAddress: txn.recipient,
                      amount: txn?.amount,
                      coinType:
                          txnEffect && getCoinType(txnEffect, txn.recipient),
                      isCoin: true,
                  },
              ]
            : null;
    }

    const paySuiData =
        getPaySuiTransaction(txnData) ?? getPayTransaction(txnData);

    const amountByRecipient = paySuiData?.recipients.reduce(
        (acc, value, index) => {
            return {
                ...acc,
                [value]: {
                    amount:
                        paySuiData.amounts[index] +
                        (value in acc ? acc[value].amount : 0),
                    coinType: txnEffect
                        ? getCoinType(
                              txnEffect,
                              paySuiData.recipients[index] ||
                                  paySuiData.recipients[0]
                          )
                        : null,
                    recipientAddress:
                        paySuiData.recipients[index] ||
                        paySuiData.recipients[0],
                    isCoin: true,
                },
            };
        },
        {} as {
            [key: string]: {
                amount: number;
                coinType: string | null;
                recipientAddress: string;
            };
        }
    );

    return amountByRecipient ? Object.values(amountByRecipient) : null;
}
