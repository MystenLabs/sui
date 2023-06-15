// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CopyContainer, Description, Heading } from './utils/ui';

export function WhatIsAWallet() {
	return (
		<CopyContainer>
			<div>
				<Heading>Easy Login</Heading>
				<Description>
					No need to create new accounts and passwords for every website. Just connect your wallet
					and get going.
				</Description>
			</div>

			<div>
				<Heading>Store your Digital Assets</Heading>
				<Description>
					Send, receive, store, and display your digital assets like NFTs & coins.
				</Description>
			</div>
		</CopyContainer>
	);
}
