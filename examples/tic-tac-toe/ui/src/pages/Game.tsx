// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import './Game.css';

import { useCurrentAccount, useSuiClient } from '@mysten/dapp-kit';
import { MultiSigPublicKey } from '@mysten/sui/multisig';
import { TrashIcon } from '@radix-ui/react-icons';
import { AlertDialog, Badge, Button, Flex } from '@radix-ui/themes';
import { Board } from 'components/Board';
import { Error } from 'components/Error';
import { IDLink } from 'components/IDLink';
import { Loading } from 'components/Loading';
import { Game as GameData, InvalidateGameQuery, Mark, useGameQuery } from 'hooks/useGameQuery';
import { useTransactions } from 'hooks/useTransactions';
import { InvalidateTrophyQuery, Trophy, useTrophyQuery } from 'hooks/useTrophyQuery';
import { useTurnCapQuery } from 'hooks/useTurnCapQuery';
import { useExecutor } from 'mutations/useExecutor';
import { ReactElement } from 'react';

type Props = {
	id: string;
};

enum Turn {
	Spectating,
	Yours,
	Theirs,
}

enum Winner {
	/** Nobody has won yet */
	None,

	/** X has won, and you are not a player */
	X,

	/** O has won, and you are not a player */
	O,

	/** You won */
	You,

	/** The other player won */
	Them,

	/** Game ended in a draw */
	Draw,
}

/**
 * Render the game at the given ID.
 *
 * Displays the noughts and crosses board, as well as a toolbar with:
 *
 * - An indicator of whose turn it is.
 * - A button to delete the game.
 * - The ID of the game being played.
 */
export default function Game({ id }: Props): ReactElement {
	const [game, invalidateGame] = useGameQuery(id);
	const [trophy, invalidateTrophy] = useTrophyQuery(game?.data);

	if (game.status === 'pending') {
		return <Loading />;
	} else if (game.status === 'error') {
		return (
			<Error title="Error loading game">
				Could not load game at <IDLink id={id} size="2" display="inline-flex" />.
				<br />
				{game.error.message}
			</Error>
		);
	}

	if (trophy.status === 'pending') {
		return <Loading />;
	} else if (trophy.status === 'error') {
		return (
			<Error title="Error loading game">
				Could not check win for <IDLink id={id} size="2" display="inline-flex" />:
				<br />
				{trophy.error.message}
			</Error>
		);
	}

	return game.data.kind === 'shared' ? (
		<SharedGame
			game={game.data}
			trophy={trophy.data}
			invalidateGame={invalidateGame}
			invalidateTrophy={invalidateTrophy}
		/>
	) : (
		<OwnedGame
			game={game.data}
			trophy={trophy.data}
			invalidateGame={invalidateGame}
			invalidateTrophy={invalidateTrophy}
		/>
	);
}

function SharedGame({
	game,
	trophy,
	invalidateGame,
	invalidateTrophy,
}: {
	game: GameData;
	trophy: Trophy;
	invalidateGame: InvalidateGameQuery;
	invalidateTrophy: InvalidateTrophyQuery;
}): ReactElement {
	const account = useCurrentAccount();
	const { mutate: signAndExecute } = useExecutor();
	const tx = useTransactions()!!;

	const { id, board, turn, x, o } = game;
	const [mark, curr, next] = turn % 2 === 0 ? [Mark.X, x, o] : [Mark.O, o, x];

	// If it's the current account's turn, then empty cells should show
	// the current player's mark on hover. Otherwise show nothing, and
	// disable interactivity.
	const player = whoseTurn({ curr, next, addr: account?.address });
	const winner = whoWon({ curr, next, addr: account?.address, turn, trophy });
	const empty = Turn.Yours === player && trophy === Trophy.None ? mark : Mark._;

	const onMove = (row: number, col: number) => {
		signAndExecute({ tx: tx.placeMark(game, row, col) }, () => {
			invalidateGame();
			invalidateTrophy();
		});
	};

	const onDelete = (andThen: () => void) => {
		signAndExecute({ tx: tx.burn(game) }, andThen);
	};

	return (
		<>
			<Board marks={board} empty={empty} onMove={onMove} />
			<Flex direction="row" gap="2" mx="2" my="6" justify="between">
				{trophy !== Trophy.None ? (
					<WinIndicator winner={winner} />
				) : (
					<MoveIndicator turn={player} />
				)}
				{trophy !== Trophy.None && account ? <DeleteButton onDelete={onDelete} /> : null}
				<IDLink id={id} />
			</Flex>
		</>
	);
}

function OwnedGame({
	game,
	trophy,
	invalidateGame,
	invalidateTrophy,
}: {
	game: GameData;
	trophy: Trophy;
	invalidateGame: InvalidateGameQuery;
	invalidateTrophy: InvalidateTrophyQuery;
}): ReactElement {
	const adminKey = game.admin ? new MultiSigPublicKey(new Uint8Array(game.admin)) : null;

	const client = useSuiClient();
	const { mutate: signAndExecute } = useExecutor();
	const { mutate: multiSignAndExecute } = useExecutor({
		execute: ({ bytes, signature }) => {
			// SAFETY: We check below whether the admin key is available,
			// and only allow moves to be submitted when it is.
			const multiSig = adminKey!!.combinePartialSignatures([signature]);
			return client.executeTransactionBlock({
				transactionBlock: bytes,
				// The multi-sig authorizes access to the game object, while
				// the original signature authorizes access to the player's
				// gas object, because the player is sponsoring the
				// transaction.
				signature: [multiSig, signature],
				options: {
					showRawEffects: true,
				},
			});
		},
	});

	const [turnCap, invalidateTurnCap] = useTurnCapQuery(game.id);
	const account = useCurrentAccount();
	const tx = useTransactions()!!;

	if (adminKey == null) {
		return (
			<Error title="Error loading game">
				Could not load game at <IDLink id={game.id} size="2" display="inline-flex" />.
				<br />
				Game has no admin.
			</Error>
		);
	}

	if (turnCap.status === 'pending') {
		return <Loading />;
	} else if (turnCap.status === 'error') {
		return (
			<Error title="Error loading game">
				Could not load turn capability.
				<br />
				{turnCap.error?.message}
			</Error>
		);
	}

	const { id, board, turn, x, o } = game;
	const [mark, curr, next] = turn % 2 === 0 ? [Mark.X, x, o] : [Mark.O, o, x];

	// If it's the current account's turn, then empty cells should show
	// the current player's mark on hover. Otherwise show nothing, and
	// disable interactivity.
	const player = whoseTurn({ curr, next, addr: account?.address });
	const winner = whoWon({ curr, next, addr: account?.address, turn, trophy });
	const empty = Turn.Yours === player && trophy === Trophy.None ? mark : Mark._;

	const onMove = (row: number, col: number) => {
		signAndExecute(
			{
				// SAFETY: TurnCap should only be unavailable if the game is over.
				tx: tx.sendMark(turnCap?.data!!, row, col),
				options: { showObjectChanges: true },
			},
			({ objectChanges }) => {
				const mark = objectChanges?.find(
					(c) => c.type === 'created' && c.objectType.endsWith('::Mark'),
				);

				if (mark && mark.type === 'created') {
					// SAFETY: UI displays error if the admin key is not
					// available, and interactivity is disabled if there is not a
					// valid account.
					//
					// The transaction to make the actual move is made by the
					// multi-sig account (which owns the game), and is sponsored
					// by the player (as the multi-sig account doesn't have coins
					// of its own).
					const recv = tx.receiveMark(game, mark);
					recv.setSender(adminKey!!.toSuiAddress());
					recv.setGasOwner(account?.address!!);

					multiSignAndExecute({ tx: recv }, () => {
						invalidateGame();
						invalidateTrophy();
						invalidateTurnCap();
					});
				}
			},
		);
	};

	const onDelete = (andThen: () => void) => {
		// Just like with making a move, deletion has to be implemented as
		// a sponsored multi-sig transaction. This means only one of the
		// two players can clean up a finished game.
		const burn = tx.burn(game);
		burn.setSender(adminKey!!.toSuiAddress());
		burn.setGasOwner(account?.address!!);

		multiSignAndExecute({ tx: burn }, andThen);
	};

	return (
		<>
			<Board marks={board} empty={empty} onMove={onMove} />
			<Flex direction="row" gap="2" mx="2" my="6" justify="between">
				{trophy !== Trophy.None ? (
					<WinIndicator winner={winner} />
				) : (
					<MoveIndicator turn={player} />
				)}
				{trophy !== Trophy.None && player !== Turn.Spectating ? (
					<DeleteButton onDelete={onDelete} />
				) : null}
				<IDLink id={id} />
			</Flex>
		</>
	);
}

/**
 * Figure out whose turn it should be based on who the `curr`ent
 * player is, who the `next` player is, and what the `addr`ess of the
 * current account is.
 */
function whoseTurn({ curr, next, addr }: { curr: string; next: string; addr?: string }): Turn {
	if (addr === curr) {
		return Turn.Yours;
	} else if (addr === next) {
		return Turn.Theirs;
	} else {
		return Turn.Spectating;
	}
}

/**
 * Figure out who won the game, out of the `curr`ent, and `next`
 * players, relative to whose asking (`addr`). `turns` indicates the
 * number of turns we've seen so far, which is used to determine which
 * address corresponds to player X and player O.
 */
function whoWon({
	curr,
	next,
	addr,
	turn,
	trophy,
}: {
	curr: string;
	next: string;
	addr?: string;
	turn: number;
	trophy: Trophy;
}): Winner {
	switch (trophy) {
		case Trophy.None:
			return Winner.None;
		case Trophy.Draw:
			return Winner.Draw;
		case Trophy.Win:
			// These tests are "backwards" because the game advances to the
			// next turn after the win has happened. Nevertheless, make sure
			// to test for the "you" case before the "them" case to handle a
			// situation where a player is playing themselves.
			if (addr === next) {
				return Winner.You;
			} else if (addr === curr) {
				return Winner.Them;
			} else if (turn % 2 === 0) {
				return Winner.O;
			} else {
				return Winner.X;
			}
	}
}

function MoveIndicator({ turn }: { turn: Turn }): ReactElement {
	switch (turn) {
		case Turn.Yours:
			return <Badge color="green">Your turn</Badge>;
		case Turn.Theirs:
			return <Badge color="orange">Their turn</Badge>;
		case Turn.Spectating:
			return <Badge color="blue">Spectating</Badge>;
	}
}

function WinIndicator({ winner }: { winner: Winner }): ReactElement | null {
	switch (winner) {
		case Winner.None:
			return null;
		case Winner.Draw:
			return <Badge color="orange">Draw!</Badge>;
		case Winner.You:
			return <Badge color="green">You Win!</Badge>;
		case Winner.Them:
			return <Badge color="red">You Lose!</Badge>;
		case Winner.X:
			return <Badge color="blue">X Wins!</Badge>;
		case Winner.O:
			return <Badge color="blue">O Wins!</Badge>;
	}
}

/**
 * "Delete" button with a confirmation dialog. On confirmation, the
 * button calls `onDelete`, passing in an action to perform after
 * deletion has completed (returning to the homepage).
 */
function DeleteButton({ onDelete }: { onDelete: (andThen: () => void) => void }): ReactElement {
	const redirect = () => {
		// Navigate back to homepage, because the game is gone now.
		window.location.href = '/';
	};

	return (
		<AlertDialog.Root>
			<AlertDialog.Trigger>
				<Button color="red" size="1" variant="outline">
					<TrashIcon /> Delete Game
				</Button>
			</AlertDialog.Trigger>
			<AlertDialog.Content>
				<AlertDialog.Title>Delete Game</AlertDialog.Title>
				<AlertDialog.Description>
					Are you sure you want to delete this game? This will delete the object from the blockchain
					and cannot be undone.
				</AlertDialog.Description>
				<Flex gap="3" mt="3" justify="end">
					<AlertDialog.Cancel>
						<Button variant="soft" color="gray">
							Cancel
						</Button>
					</AlertDialog.Cancel>
					<AlertDialog.Action onClick={() => onDelete(redirect)}>
						<Button variant="solid" color="red">
							Delete
						</Button>
					</AlertDialog.Action>
				</Flex>
			</AlertDialog.Content>
		</AlertDialog.Root>
	);
}
