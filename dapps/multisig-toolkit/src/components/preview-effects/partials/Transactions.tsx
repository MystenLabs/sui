// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiArgument, SuiCallArg, SuiTransaction, TransactionBlockData } from '@mysten/sui/client';
import { ReactNode } from 'react';

import { ObjectLink } from '../ObjectLink';
import { PreviewCard } from '../PreviewCard';

export function Transactions({ inputs }: { inputs: TransactionBlockData }) {
	if (inputs.transaction.kind !== 'ProgrammableTransaction') return null;

	return (
		<div className="">
			{inputs.transaction.transactions.map((transaction, index) => (
				<Transaction key={index} transaction={transaction} inputs={inputs} index={index} />
			))}
		</div>
	);
}

const getCallArgDisplay = (argument: SuiCallArg | undefined) => {
	if (!argument) return null;
	if (typeof argument === 'string') return argument;

	return (
		<PreviewCard.Root>
			<PreviewCard.Body>
				{Object.entries(argument)
					.filter(([key, value]) => value !== null)
					.map(([key, value]) => (
						<div key={key} className="flex items-center flex-shrink-0 gap-3 mb-3 justify-stretch ">
							<p className="capitalize min-w-[100px] flex-shrink-0">{key}: </p>
							{key === 'objectId' ? (
								<ObjectLink inputObject={value as string} />
							) : typeof value === 'object' ? (
								JSON.stringify(value)
							) : (
								(value as ReactNode)
							)}
						</div>
					))}
			</PreviewCard.Body>
		</PreviewCard.Root>
	);
};

const getSuiArgumentDisplay = (argument: SuiArgument, inputs: SuiCallArg[]) => {
	if (typeof argument === 'string') return argument;

	if ('Input' in argument) {
		return getCallArgDisplay(inputs[argument.Input]);
	}

	return (
		<PreviewCard.Root>
			<PreviewCard.Body>{JSON.stringify(argument)}</PreviewCard.Body>
		</PreviewCard.Root>
	);
};

const renderArguments = (callArgs: SuiArgument[], inputs: SuiCallArg[]) => {
	return (
		<div className="flex overflow-x-auto gap-3 my-3">
			{callArgs.map((arg, index) => (
				<div key={index} className="flex-shrink-0">
					{getSuiArgumentDisplay(arg, inputs)}
				</div>
			))}
		</div>
	);
};

const renderFooter = (type: string, index: number) => {
	return (
		<PreviewCard.Header>
			<p>
				{index}. Type: <strong>{type}</strong>
			</p>
		</PreviewCard.Header>
	);
};

const FOOTERS: Record<string, string> = {
	MoveCall: "MoveCall (Direct Call to a Smart Contract's function)",
	TransferObjects: 'TransferObjects (transfers the list of objects to the specified address)',
	SplitCoins: 'SplitCoins (splits the coin into multiple coins)',
	MergeCoins: 'MergeCoins (merges the coins into a single coin)',
	Publish: 'Publish (publishes a new package)',
	Upgrade: 'Upgrade (upgrades a package)',
	MakeMoveVec: 'MakeMoveVec (creates a vector of Move objects)',
};

function Transaction({
	transaction,
	inputs,
	index,
}: {
	transaction: SuiTransaction;
	inputs: TransactionBlockData;
	index: number;
}) {
	const inputList = () => {
		if (inputs.transaction.kind === 'ProgrammableTransaction') {
			return inputs.transaction.inputs;
		}
		return [];
	};

	return (
		<PreviewCard.Root className="mb-6">
			{renderFooter(FOOTERS[Object.keys(transaction)[0]], index)}
			<PreviewCard.Body>
				<>
					{'MoveCall' in transaction && (
						<>
							<div className="mb-3">
								Target:{' '}
								{`${transaction.MoveCall.package}::${transaction.MoveCall.module}::${transaction.MoveCall.function}`}
							</div>
							{transaction.MoveCall.type_arguments &&
								transaction.MoveCall.type_arguments.length > 0 && (
									<div className="mb-3">
										<label>Type Arguments: </label>[{transaction.MoveCall.type_arguments.join(', ')}
										]
									</div>
								)}

							{transaction.MoveCall.arguments && transaction.MoveCall.arguments.length > 0 && (
								<div>
									<label>Inputs: </label>
									{renderArguments(transaction.MoveCall.arguments, inputList())}
								</div>
							)}
						</>
					)}

					{'TransferObjects' in transaction && (
						<>
							<div>
								<label>Objects: </label>
								{renderArguments(transaction.TransferObjects[0], inputList())}

								<label>Transfer to:</label>
								{renderArguments([transaction.TransferObjects[1]], inputList())}
							</div>
						</>
					)}

					{'SplitCoins' in transaction && (
						<>
							<div>
								<label>From Coin: </label>
								{renderArguments([transaction.SplitCoins[0]], inputList())}
							</div>
							<div>
								<label>Splits into: </label>
								{renderArguments(transaction.SplitCoins[1], inputList())}
							</div>
						</>
					)}

					{'MergeCoins' in transaction && (
						<>
							<div>
								<label>To Coin: </label>
								{renderArguments([transaction.MergeCoins[0]], inputList())}
							</div>
							<div>
								<label>From coins: </label>
								{renderArguments(transaction.MergeCoins[1], inputList())}
							</div>
						</>
					)}

					{('Publish' in transaction || 'Upgrade' in transaction) && (
						<>{JSON.stringify(transaction)}</>
					)}

					{'MakeMoveVec' in transaction && (
						<>
							{/* TODO: Create a sample tx with MakeMoveVec to render this better. */}
							<div>
								<label>Objects: </label>
								{JSON.stringify(transaction.MakeMoveVec)}
							</div>
						</>
					)}
				</>
			</PreviewCard.Body>
		</PreviewCard.Root>
	);
}
