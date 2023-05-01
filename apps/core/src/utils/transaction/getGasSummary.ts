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
} from '@mysten/sui.js';

export type GasSummaryType =
    | (GasCostSummary & {
          totalGas?: string;
          owner?: string;
          isSponsored: boolean;
      })
    | null;

export function getGasSummary(
    transaction: SuiTransactionBlockResponse | DryRunTransactionBlockResponse
) {
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

    return {
        ...effects.gasUsed,
        owner,
        totalGas: totalGas?.toString(),
        isSponsored: !!owner && !!sender && owner !== sender,
    };
}
