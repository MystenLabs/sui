// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Heading } from '../ui/Heading.js';
import { Text } from '../ui/Text.js';
import * as styles from './InfoSection.css.js';

type InfoSectionProps = {
	title: string;
	children: string;
};

export function InfoSection({ title, children }: InfoSectionProps) {
	return (
		<section className={styles.container}>
			<Heading as="h3" size="sm" weight="normal">
				{title}
			</Heading>
			<Text weight="medium" color="muted">
				{children}
			</Text>
		</section>
	);
}
