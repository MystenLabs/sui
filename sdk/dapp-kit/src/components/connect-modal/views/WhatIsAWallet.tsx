// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Heading } from '../../ui/Heading.js';
import { InfoSection } from '../InfoSection.js';
import * as styles from './WhatIsAWallet.css.js';

export function WhatIsAWallet() {
	return (
		<div className={styles.container}>
			<Heading as="h2">What is a Wallet</Heading>
			<div className={styles.content}>
				<InfoSection title="Easy Login">
					No need to create new accounts and passwords for every website. Just connect your wallet
					and get going.
				</InfoSection>
				<InfoSection title="Store your Digital Assets">
					Send, receive, store, and display your digital assets like NFTs & coins.
				</InfoSection>
			</div>
		</div>
	);
}
