// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { InfoSection } from '../InfoSection.js';
import * as styles from './GettingStarted.css.js';

export function GettingStarted() {
	return (
		<div className={styles.container}>
			<h2 className={styles.title}>Get Started with Sui</h2>
			<div className={styles.content}>
				<InfoSection title="Install the Sui Wallet Extension">
					We recommend pinning Sui Wallet to your taskbar for quicker access.
				</InfoSection>
				<InfoSection title="Create or Import a Wallet">
					Be sure to back up your wallet using a secure method. Never share your secret phrase with
					anyone.
				</InfoSection>
				<InfoSection title="Refresh Your Browser">
					Once you set up your wallet, refresh this window browser to load up the extension.
				</InfoSection>
				<a
					className={styles.installLink}
					href="https://chrome.google.com/webstore/detail/sui-wallet/opcgpfmipidbgpenhmajoajpbobppdil"
					target="_blank"
					rel="noreferrer"
				>
					Install Wallet Extension
				</a>
			</div>
		</div>
	);
}
