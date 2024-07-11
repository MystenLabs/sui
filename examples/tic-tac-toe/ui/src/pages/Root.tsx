// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { GlobeIcon, LockClosedIcon } from '@radix-ui/react-icons';
import { Container, Heading, Tabs, Text } from '@radix-ui/themes';
import { NewMultiSigGame } from 'components/NewMultiSigGame';
import { NewSharedGame } from 'components/NewSharedGame';
import { ReactElement } from 'react';

/**
 * Landing page for the root path. Displays a form for creating a new game.
 */
export default function Root(): ReactElement {
	return (
		<Container m="2">
			<Heading size="9" mb="9">
				New game
			</Heading>
			<Tabs.Root defaultValue="owned">
				<Tabs.List mb="4">
					<Kind value="owned" icon={<LockClosedIcon />} label="Multi-sig" />
					<Kind value="shared" icon={<GlobeIcon />} label="Shared" />
				</Tabs.List>
				<Tabs.Content value="owned">
					<NewMultiSigGame />
				</Tabs.Content>
				<Tabs.Content value="shared">
					<NewSharedGame />
				</Tabs.Content>
			</Tabs.Root>
		</Container>
	);
}

/**
 * Re-usable component for defining a single "kind" tab.
 *
 * Each "kind" option is a tab, with a value, icon, and label. The tab
   content is handled separately.
 */
function Kind({
	value,
	icon,
	label,
}: {
	value: string;
	icon: ReactElement;
	label: string;
}): ReactElement {
	return (
		<Tabs.Trigger value={value}>
			{icon}
			<Text ml="1">{label}</Text>
		</Tabs.Trigger>
	);
}
