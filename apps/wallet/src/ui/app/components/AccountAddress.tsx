// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useResolveSuiNSName } from '@mysten/core';
import { Copy12 } from '@mysten/icons';
import { formatAddress } from '@mysten/sui.js';

import { useActiveAddress } from '../hooks/useActiveAddress';
import { useCopyToClipboard } from '../hooks/useCopyToClipboard';
import { Text } from '../shared/text';

type AccountAddressProps = {
	copyable?: boolean;
	address?: string;
};

export function AccountAddress({ copyable, address }: AccountAddressProps) {
	const activeAddress = useActiveAddress();
	const addressToShow = address || activeAddress;
	const copyCallback = useCopyToClipboard(addressToShow || '', {
		copySuccessMessage: 'Address copied',
	});

	const { data: domainName } = useResolveSuiNSName(addressToShow);

	return addressToShow ? (
		<div className="flex flex-nowrap flex-row items-center gap-1">
			<Text variant="bodySmall" weight="medium" color="sui-dark" mono>
				{domainName ?? formatAddress(addressToShow)}
			</Text>
			{copyable ? <Copy12 className="text-steel cursor-pointer" onClick={copyCallback} /> : null}
		</div>
	) : null;
}
