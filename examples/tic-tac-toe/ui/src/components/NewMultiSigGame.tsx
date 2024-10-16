// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCurrentAccount } from '@mysten/dapp-kit';
import { PublicKey } from '@mysten/sui/cryptography';
import { fromBase64, toBase64 } from '@mysten/sui/utils';
import { publicKeyFromRawBytes } from '@mysten/sui/verify';
import { ExclamationTriangleIcon } from '@radix-ui/react-icons';
import { Box, Button, Em, Flex, Separator, Spinner, Text, TextField } from '@radix-ui/themes';
import { ComputedField } from 'components/ComputedField';
import { useTransactions } from 'hooks/useTransactions';
import { useExecutor } from 'mutations/useExecutor';
import { ReactElement, useState } from 'react';

/**
 * Form for creating a new multi-sig game.
 */
export function NewMultiSigGame(): ReactElement {
	// SAFETY: <App /> tests that a package exists, so Transactions
	// builder should be available.
	const tx = useTransactions()!!;
	const { mutate: signAndExecute, isPending } = useExecutor();

	const { address, publicKey: bytes } = useCurrentAccount() || {};
	const [opponent, setOpponent] = useState<PublicKey | null>(null);

	const publicKey = bytes && publicKeyFromRawBytes('ED25519', bytes);
	const hasPlayer = publicKey != null;
	const hasOpponent = opponent != null;

	function onClick() {
		signAndExecute(
			{
				// SAFETY: Button is only enabled when player and opponent are
				// available.
				tx: tx.newMultiSigGame(publicKey!!, opponent!!),
				options: { showObjectChanges: true },
			},
			({ objectChanges }) => {
				const game = objectChanges?.find(
					(c) => c.type === 'created' && c.objectType.endsWith('::Game'),
				);

				if (game && game.type === 'created') {
					window.location.href = `/${game.objectId}`;
				}
			},
		);
	}

	return (
		<>
			<ComputedField value={bytes && toBase64(bytes)} label="Your public key" />
			<ComputedField value={address} label="Your address" />
			<TextField.Root
				size="2"
				mb="2"
				placeholder="Opponent public key"
				style={{ width: '100%' }}
				color={hasOpponent ? undefined : 'red'}
				variant={hasOpponent ? 'surface' : 'soft'}
				onChange={(e) => setOpponent(parsePublicKey(e.target.value))}
			/>
			<ComputedField
				value={opponent ? opponent.toSuiAddress() : undefined}
				label="Opponent address"
			/>
			<Flex justify="between" mt="4">
				<Validation hasPlayer={hasPlayer} hasOpponent={hasOpponent} />
				<Button
					variant="outline"
					disabled={!(publicKey && opponent) || isPending}
					onClick={onClick}
				>
					{isPending ? <Spinner /> : null} Play
				</Button>
			</Flex>
			<Separator orientation="horizontal" my="4" style={{ width: '100%' }} />
			<Em>
				<Text as="div" mb="2">
					Create a 1-of-2 multi-sig address to own the new game. Each move in the game requires two
					fast path (single-owner) transactions: One from the player's address to authorize the
					move, and one from the multi-sig, signed by the player's address, to make the move.
				</Text>
				<Text as="div">
					In order to construct the multi-sig, we need to know the public keys of the two players.
					Although addresses on Sui are derived from public keys, the derivation cannot be reversed,
					so to start a multi-sig game, we ask for the public keys directly.
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
				<Text color="red">Invalid opponent public key.</Text>
			</Flex>
		);
	}

	return <Box />;
}

/**
 * If `key` is a valid base64 encoded Sui public key, return it as a
 * `PublicKey`, otherwise return null.
 */
function parsePublicKey(key?: string): PublicKey | null {
	if (key == null) {
		return null;
	}

	key = key.trim();
	if (key === '') {
		return null;
	}

	try {
		return publicKeyFromRawBytes('ED25519', fromBase64(key));
	} catch (e) {
		console.error('Failed to get public key from raw bytes', e);
		return null;
	}
}
