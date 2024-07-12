// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Flex, Spinner, Text } from '@radix-ui/themes';
import { Board } from 'components/Board';
import { Mark } from 'hooks/useGameQuery';

/**
 * Component designed to represent the null-state of a Game, to be
 * used as a loading screen while the game is being fetched from
 * RPC.
 */
export function Loading() {
	return (
		<>
			<Board
				marks={Array.from({ length: 9 }, () => Mark._)}
				empty={Mark._}
				onMove={(_r, _c) => {}}
			/>
			<Flex gap="1" align="center" justify="center" mx="2" my="6">
				<Spinner size="3" />
				<Text>Loading...</Text>
			</Flex>
		</>
	);
}
