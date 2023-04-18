// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiNSName, useRpcClient } from '@mysten/core';
import { type JsonRpcProvider, isValidSuiAddress } from '@mysten/sui.js';
import { useMemo } from 'react';
import * as Yup from 'yup';

export function createSuiAddressValidation(rpc: JsonRpcProvider) {
    return Yup.string()
        .ensure()
        .trim()
        .required()
        .test(
            'is-sui-address',
            'Invalid address. Please check again.',
            async (value) => {
                if (isSuiNSName(value)) {
                    // TODO: Remove:
                    return true;
                    const address = await rpc.resolveNameServiceAddress({
                        name: value,
                    });

                    return !!address;
                }

                return isValidSuiAddress(value);
            }
        )
        .label("Recipient's address");
}

export function useSuiAddressValidation() {
    const rpc = useRpcClient();

    return useMemo(() => {
        return createSuiAddressValidation(rpc);
    }, [rpc]);
}
