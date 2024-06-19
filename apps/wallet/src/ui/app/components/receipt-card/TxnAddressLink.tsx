// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import { useResolveSuiNSAddress } from '@mysten/core';
import { formatAddress, isValidSuiNSName } from '@mysten/sui/utils';

type TxnAddressLinkProps = {
	address: string;
};

export function TxnAddressLink({ address }: TxnAddressLinkProps) {
	const { data: resolvedAddress } = useResolveSuiNSAddress(address);
	return (
		<ExplorerLink
			type={ExplorerLinkType.address}
			address={isValidSuiNSName(address) && resolvedAddress ? resolvedAddress : address}
			title="View on Sui Explorer"
			showIcon={false}
		>
			{isValidSuiNSName(address) ? address : formatAddress(address)}
		</ExplorerLink>
	);
}
