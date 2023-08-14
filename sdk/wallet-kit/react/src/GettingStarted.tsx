// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CopyContainer, Description, Heading } from './utils/ui';

export function GettingStarted() {
	return (
		<CopyContainer>
			<div>
				<Heading>Install the Sui extension</Heading>
				<Description>
					We recommend pinning the Sui Wallet to your taskbar for quicker access.
				</Description>
			</div>

			<div>
				<Heading>Create or Import a Wallet</Heading>
				<Description>
					Be sure to back up your wallet using a secure method. Never share your secret phrase with
					anyone.
				</Description>
			</div>

			<div>
				<Heading>Refresh Your Browser</Heading>
				<Description>
					Once you set up your wallet, refresh this window browser to load up the extension.
				</Description>
			</div>
		</CopyContainer>
	);
}
