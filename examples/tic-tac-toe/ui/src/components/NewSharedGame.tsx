// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCurrentAccount } from '@mysten/dapp-kit';
import { isValidSuiAddress, normalizeSuiAddress } from '@mysten/sui/utils';
import { ExclamationTriangleIcon } from '@radix-ui/react-icons';
import { Box, Button, Em, Flex, Separator, Spinner, Text, TextField } from '@radix-ui/themes';
import { useTransactions } from 'hooks/useTransactions';
import { useExecutor } from 'mutations/useExecutor';
import { ReactElement, useState } from 'react';

import { ComputedField } from './ComputedField';

/**
 * Form for creating a new shared game.
 */
export function NewSharedGame(): ReactElement {
	// SAFETY: <App /> tests that a package exists, so Transactions
	// builder should be available.
	const tx = useTransactions()!!;
	const { mutate: signAndExecute, isPending } = useExecutor();

	const player = useCurrentAccount()?.address;
	const [opponent, setOpponent] = useState<string | null>(null);

	const hasPlayer = player != null;
	const hasOpponent = opponent != null;

	function onClick() {
		signAndExecute(
			{
				// SAFETY: Button is only enabled when player and opponent are
				// available.
				tx: tx.newSharedGame(player!!, opponent!!),
				options: { showEffects: true },
			},
			({ effects }) => {
				const gameId = effects?.created?.[0].reference?.objectId;
				if (gameId) {
					window.location.href = `/${gameId}`;
				}
			},
		);
	}

	return (
		<>
			<ComputedField label="Your address" value={player} />
			<TextField.Root
				size="2"
				mb="2"
				placeholder="Opponent address"
				style={{ width: '100%' }}
				color={hasOpponent ? undefined : 'red'}
				variant={hasOpponent ? 'surface' : 'soft'}
				onChange={(e) => setOpponent(normalizedAddress(e.target.value))}
			/>
			<Flex justify="between" mt="4">
				<Validation hasPlayer={hasPlayer} hasOpponent={hasOpponent} />
				<Button variant="outline" disabled={!(player && opponent) || isPending} onClick={onClick}>
					{isPending ? <Spinner /> : null} Play
				</Button>
			</Flex>
			<Separator orientation="horizontal" my="4" style={{ width: '100%' }} />
			<Em>
				<Text as="div">
					Create the game as a shared object that both players can access. Each move is a single
					transaction, but it requires going through consensus.
				</Text>
			</Em>
		</>
	);
}

function Validation({
	hasPlayer,
	hasOpponent,
}: {
	hasPlayer: boolean;
	hasOpponent: boolean;
}): ReactElement {
	if (!hasPlayer) {
		return (
			<Flex align="center" gap="2">
				<ExclamationTriangleIcon color="red" />
				<Text color="red">Wallet not connected.</Text>
			</Flex>
		);
	}

	if (!hasOpponent) {
		return (
			<Flex align="center" gap="2">
				<ExclamationTriangleIcon color="red" />
				<Text color="red">Invalid opponent address.</Text>
			</Flex>
		);
	}

	return <Box />;
}

/**
 * If `address` is a valid denormalized address, return it in its
 * normalized form, and otherwise return null.
 */
function normalizedAddress(address?: string): string | null {
	if (address == null) {
		return null;
	}

	address = address.trim();
	if (address === '') {
		return null;
	}

	address = normalizeSuiAddress(address);
	return isValidSuiAddress(address) ? address : null;
}
