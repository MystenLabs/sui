// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as styles from './WhatIsAWallet.css.js';
import { InfoSection } from '../InfoSection.js';

export function WhatIsAWallet() {
	return (
		<div className={styles.container}>
			<h2 className={styles.title}>What is a Wallet</h2>
			<InfoSection title="Easy Login">
				No need to create new accounts and passwords for every website. Just connect your wallet and
				get going.
			</InfoSection>
			<InfoSection title="Store your Digital Assets">
				Send, receive, store, and display your digital assets like NFTs & coins.
			</InfoSection>
		</div>
	);
}
