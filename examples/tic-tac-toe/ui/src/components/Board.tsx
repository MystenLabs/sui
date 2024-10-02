// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CircleIcon, Cross1Icon } from '@radix-ui/react-icons';
import { Box, Flex } from '@radix-ui/themes';
import { Mark } from 'hooks/useGameQuery';
import { ReactElement } from 'react';

/**
 * Represents a Tic-Tac-Toe board.
 *
 * `marks` is a linear array containing the marks on the board, in
 * row-major order, `empty` is the Mark to display when hovering over
 * an empty cell, and `onMove` is a callback to be called when an
 * empty cell is clicked. Setting `empty` to `Mark._` will make empty
 * cells non-interactive.
 */
export function Board({
	marks,
	empty,
	onMove,
}: {
	marks: Mark[];
	empty: Mark;
	onMove: (i: number, j: number) => void;
}): ReactElement {
	const board = Array.from({ length: 3 }, (_, i) => marks.slice(i * 3, (i + 1) * 3));

	return (
		<Flex direction="column" gap="2" className="board" mb="2">
			{board.map((row, r) => (
				<Flex direction="row" gap="2" key={r}>
					{row.map((cell, c) => (
						<Cell key={c} mark={cell} empty={empty} onMove={() => onMove(r, c)} />
					))}
				</Flex>
			))}
		</Flex>
	);
}

function Cell({
	mark,
	empty,
	onMove,
}: {
	mark: Mark;
	empty: Mark;
	onMove: () => void;
}): ReactElement {
	switch (mark) {
		case Mark.X:
			return <Cross1Icon className="cell" width="100%" height="100%" />;
		case Mark.O:
			return <CircleIcon className="cell" width="100%" height="100%" />;
		case Mark._:
			return <EmptyCell empty={empty} onMove={onMove} />;
	}
}

function EmptyCell({ empty, onMove }: { empty: Mark; onMove: () => void }): ReactElement | null {
	switch (empty) {
		case Mark.X:
			return <Cross1Icon className="cell empty" width="100%" height="100%" onClick={onMove} />;
		case Mark.O:
			return <CircleIcon className="cell empty" width="100%" height="100%" onClick={onMove} />;
		case Mark._:
			return <Box className="cell empty" width="100%" height="100%" />;
	}
}
