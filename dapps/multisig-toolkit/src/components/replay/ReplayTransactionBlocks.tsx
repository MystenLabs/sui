// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { PreviewCard } from '../preview-effects/PreviewCard';
import {
	Argument,
	MoveValue,
	MutableReference,
	ReplayProgrammableTransactions,
} from './replay-types';
import { ReplayInputArgument } from './ReplayInputArgument';
import { ReplayLink } from './ReplayLink';

export function ReplayTransactionBlocks({
	transactions,
}: {
	transactions: ReplayProgrammableTransactions;
}) {
	const renderArgument = (argument: Argument | string) => {
		if (typeof argument !== 'string' && 'Input' in argument && argument.Input !== undefined) {
			return <ReplayInputArgument input={transactions.inputs[argument.Input]} />;
		}

		return (
			<PreviewCard.Root>
				<PreviewCard.Body>{JSON.stringify(argument)}</PreviewCard.Body>
			</PreviewCard.Root>
		);
	};
	const renderMRef = (ref: [Argument, MoveValue]) => {
		return (
			<PreviewCard.Root>
				<PreviewCard.Body>
					<div>{renderArgument(ref[0])}</div>
				</PreviewCard.Body>
			</PreviewCard.Root>
		);
	};

	return (
		<div>
			{transactions.commands.map((commandWithOutput, index) => (
				<PreviewCard.Root className="mb-3">
					<PreviewCard.Header>
						<p className="font-bold">
							{index}. {Object.keys(commandWithOutput.command)[0]}
						</p>
						{'MoveCall' in commandWithOutput.command && (
							<div className="flex flex-wrap gap-3">
								Package:{' '}
								<ReplayLink
									landing={true}
									id={commandWithOutput.command.MoveCall.package}
									text={commandWithOutput.command.MoveCall.package}
								/>
								Module:{' '}
								<ReplayLink
									landing={true}
									id={commandWithOutput.command.MoveCall.package}
									module={commandWithOutput.command.MoveCall.module}
									text={commandWithOutput.command.MoveCall.module}
								/>
								Function: {commandWithOutput.command.MoveCall.function}
							</div>
						)}
					</PreviewCard.Header>
					<PreviewCard.Body>
						<div className="max-h-[300px] overflow-y-auto grid grid-cols-1 gap-2">
							{'MoveCall' in commandWithOutput.command &&
								commandWithOutput.command.MoveCall.arguments.map((argument) =>
									renderArgument(argument),
								)}

							{'SplitCoins' in commandWithOutput.command && (
								<>
									<div>Coin: {renderArgument(commandWithOutput.command.SplitCoins[0])}</div>
									<div>
										Amounts:
										{commandWithOutput.command.SplitCoins[1].map((argument) =>
											renderArgument(argument),
										)}
									</div>
								</>
							)}

							{'TransferObjects' in commandWithOutput.command && (
								<>
									<div>Address: {renderArgument(commandWithOutput.command.TransferObjects[1])}</div>
									<div>
										Objects:
										{commandWithOutput.command.TransferObjects[0].map((argument) =>
											renderArgument(argument),
										)}
									</div>
								</>
							)}

							{'MergeCoins' in commandWithOutput.command && (
								<>
									<div>Coin: {renderArgument(commandWithOutput.command.MergeCoins[0])}</div>
									<div>
										Coins:
										{commandWithOutput.command.MergeCoins[1].map((argument) =>
											renderArgument(argument),
										)}
									</div>
								</>
							)}
							{'MakeMoveVec' in commandWithOutput.command && (
								<>
									<div>
										Arguments:
										{commandWithOutput.command.MakeMoveVec[1].map((argument) =>
											renderArgument(argument),
										)}
									</div>
								</>
							)}
							{'Publish' in commandWithOutput.command && (
								<>
									<div>
										ObjectIds:{' '}
										{commandWithOutput.command.Publish[1].map((argument) =>
											renderArgument(argument),
										)}
									</div>
								</>
							)}
							{'Upgrade' in commandWithOutput.command && (
								<>
									<div>ObjectId: {renderArgument(commandWithOutput.command.Upgrade[2])}</div>
								</>
							)}
						</div>
					</PreviewCard.Body>
					<PreviewCard.Body>
						<div>
							Mutable Reference Outputs:{' '}
							{commandWithOutput.MutableRefs.map((ref) => renderMRef(ref))}
						</div>
					</PreviewCard.Body>
				</PreviewCard.Root>
			))}
		</div>
	);
}
