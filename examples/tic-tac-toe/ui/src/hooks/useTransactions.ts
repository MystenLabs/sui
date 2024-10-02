// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '@mysten/sui/bcs';
import { PublicKey } from '@mysten/sui/cryptography';
import { ObjectRef, Transaction } from '@mysten/sui/transactions';
import { useNetworkVariable } from 'config';
import { Game } from 'hooks/useGameQuery';
import { TurnCap } from 'hooks/useTurnCapQuery';
import { multiSigPublicKey } from 'MultiSig';

/** Hook to provide an instance of the Transactions builder. */
export function useTransactions(): Transactions | null {
	const packageId = useNetworkVariable('packageId');
	return packageId ? new Transactions(packageId) : null;
}

/**
 * Builds on-chain transactions for the Tic-Tac-Toe game.
 */
export class Transactions {
	readonly packageId: string;

	constructor(packageId: string) {
		this.packageId = packageId;
	}

	newSharedGame(player: string, opponent: string): Transaction {
		const tx = new Transaction();

		tx.moveCall({
			target: `${this.packageId}::shared::new`,
			arguments: [tx.pure.address(player), tx.pure.address(opponent)],
		});

		return tx;
	}

	newMultiSigGame(player: PublicKey, opponent: PublicKey): Transaction {
		const admin = multiSigPublicKey([player, opponent]);
		const tx = new Transaction();

		const game = tx.moveCall({
			target: `${this.packageId}::owned::new`,
			arguments: [
				tx.pure.address(player.toSuiAddress()),
				tx.pure.address(opponent.toSuiAddress()),
				tx.pure(bcs.vector(bcs.u8()).serialize(admin.toRawBytes()).toBytes()),
			],
		});

		tx.transferObjects([game], admin.toSuiAddress());

		return tx;
	}

	placeMark(game: Game, row: number, col: number): Transaction {
		if (game.kind !== 'shared') {
			throw new Error('Cannot place mark directly on owned game');
		}

		const tx = new Transaction();

		tx.moveCall({
			target: `${this.packageId}::shared::place_mark`,
			arguments: [tx.object(game.id), tx.pure.u8(row), tx.pure.u8(col)],
		});

		return tx;
	}

	sendMark(cap: TurnCap, row: number, col: number): Transaction {
		const tx = new Transaction();

		tx.moveCall({
			target: `${this.packageId}::owned::send_mark`,
			arguments: [tx.object(cap.id.id), tx.pure.u8(row), tx.pure.u8(col)],
		});

		return tx;
	}

	receiveMark(game: Game, mark: ObjectRef): Transaction {
		if (game.kind !== 'owned') {
			throw new Error('Cannot receive mark on shared game');
		}

		const tx = new Transaction();

		tx.moveCall({
			target: `${this.packageId}::owned::place_mark`,
			arguments: [tx.object(game.id), tx.receivingRef(mark)],
		});

		return tx;
	}

	ended(game: Game): Transaction {
		const tx = new Transaction();

		tx.moveCall({
			target: `${this.packageId}::${game.kind}::ended`,
			arguments: [tx.object(game.id)],
		});

		return tx;
	}

	burn(game: Game): Transaction {
		const tx = new Transaction();

		tx.moveCall({
			target: `${this.packageId}::${game.kind}::burn`,
			arguments: [tx.object(game.id)],
		});

		return tx;
	}
}
