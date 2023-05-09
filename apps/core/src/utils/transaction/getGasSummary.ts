// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
    DryRunTransactionBlockResponse,
    GasCostSummary,
    SuiTransactionBlockResponse,
    getGasData,
    getTotalGasUsed,
    getTransactionSender,
    is,
    SuiGasData,
} from '@mysten/sui.js';

type Optional<T> = {
    [K in keyof T]?: T[K];
};

export type GasSummaryType =
    | (GasCostSummary &
          Optional<SuiGasData> & {
              totalGas?: string;
              owner?: string;
              isSponsored: boolean;
              gasUsed: GasCostSummary;
          })
    | null;

export function getGasSummary(
    transaction: SuiTransactionBlockResponse | DryRunTransactionBlockResponse
): GasSummaryType {
    const { effects } = transaction;
    if (!effects) return null;
    const totalGas = getTotalGasUsed(effects);

    let sender = is(transaction, SuiTransactionBlockResponse)
        ? getTransactionSender(transaction)
        : undefined;

    const owner = is(transaction, SuiTransactionBlockResponse)
        ? getGasData(transaction)?.owner
        : typeof effects.gasObject.owner === 'object' &&
          'AddressOwner' in effects.gasObject.owner
        ? effects.gasObject.owner.AddressOwner
        : '';

    const gasData = is(transaction, SuiTransactionBlockResponse)
        ? getGasData(transaction)
        : {};

    return {
        ...effects.gasUsed,
        ...gasData,
        owner,
        totalGas: totalGas?.toString(),
        isSponsored: !!owner && !!sender && owner !== sender,
        gasUsed: transaction?.effects!.gasUsed,
    };
}
