// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	BuilderCallArg,
	formatAddress,
	is,
	toB64,
	type TransactionBlockInput,
} from '@mysten/sui.js';

import ExplorerLink from '_src/ui/app/components/explorer-link';
import { ExplorerLinkType } from '_src/ui/app/components/explorer-link/ExplorerLinkType';
import { Text } from '_src/ui/app/shared/text';

interface InputProps {
	input: TransactionBlockInput;
}

export function Input({ input }: InputProps) {
	const { objectId } = input.value?.Object?.ImmOrOwned || input.value?.Object?.Shared || {};

	return (
		<div className="break-all">
			<Text variant="pBodySmall" weight="medium" color="steel-dark" mono>
				{is(input.value, BuilderCallArg) ? (
					'Pure' in input.value ? (
						`${toB64(new Uint8Array(input.value.Pure))}`
					) : (
						<ExplorerLink
							className="text-hero-dark no-underline"
							type={ExplorerLinkType.object}
							objectID={objectId}
						>
							{formatAddress(objectId)}
						</ExplorerLink>
					)
				) : (
					'Unknown input value'
				)}
			</Text>
		</div>
	);
}
