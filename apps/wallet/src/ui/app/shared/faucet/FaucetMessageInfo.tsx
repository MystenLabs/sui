// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';
import { useFormatCoin } from '@mysten/core';

export type FaucetMessageInfoProps = {
	error?: string | null;
	loading?: boolean;
	totalReceived?: number | null;
};

function FaucetMessageInfo({
	error = null,
	loading = false,
	totalReceived = null,
}: FaucetMessageInfoProps) {
	const [coinsReceivedFormatted, coinsReceivedSymbol] = useFormatCoin(totalReceived, GAS_TYPE_ARG);
	if (loading) {
		return <>Request in progress</>;
	}
	if (error) {
		return <>{error}</>;
	}
	return (
		<>{`${totalReceived ? `${coinsReceivedFormatted} ` : ''}${coinsReceivedSymbol} received`}</>
	);
}

export default FaucetMessageInfo;
