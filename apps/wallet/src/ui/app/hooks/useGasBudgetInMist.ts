// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useReferenceGasPrice } from './useReferenceGasPrice';

/**
 * Converts the gas units in Mist based on reference gas price of the current epoch.
 * Wallet currently calculates all the estimated gas budget in gas units, we use this
 * hook to convert it to mist.
 */
export function useGasBudgetInMist(gasBudgetUnits: number) {
    const {
        data: gasPrice,
        isError,
        isLoading,
        isSuccess,
    } = useReferenceGasPrice();
    let gasBudget = undefined;
    if (isSuccess && gasPrice) {
        gasBudget = gasBudgetUnits * gasPrice;
    }
    if (isError) {
        gasBudget = gasBudgetUnits;
    }
    return {
        gasBudget,
        isLoading,
        isError,
        isSuccess,
    };
}
