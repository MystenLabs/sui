// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// import { useFormatCoin } from '@mysten/core';

// import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';

export type FaucetMessageInfoProps = {
    error?: string | null;
    loading?: boolean;
    task?: string | null;
};

function FaucetMessageInfo({
    error = null,
    loading = false,
    task = null,
}: FaucetMessageInfoProps) {
    if (loading) {
        return <>Request in progress</>;
    }
    if (error) {
        return <>{error}</>;
    }
    if (!task) {
        return <>Try faucet request again!</>;
    }
    return <>Faucet request in progress.</>;
}

export default FaucetMessageInfo;
