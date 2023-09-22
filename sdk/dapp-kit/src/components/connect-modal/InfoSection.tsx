// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as styles from './InfoSection.css.js';

type InfoSectionProps = {
	title: string;
	children: string;
};

export function InfoSection({ title, children }: InfoSectionProps) {
	return (
		<section className={styles.container}>
			<h3 className={styles.heading}>{title}</h3>
			<div className={styles.description}>{children}</div>
		</section>
	);
}
