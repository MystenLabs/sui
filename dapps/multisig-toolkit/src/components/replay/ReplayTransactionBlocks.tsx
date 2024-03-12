// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { PreviewCard } from '../preview-effects/PreviewCard';
import { Argument, ReplayProgrammableTransactions } from './replay-types';
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
	return (
		<div>
			{transactions.commands.map((command, index) => (
				<PreviewCard.Root className="mb-3">
					<PreviewCard.Header>
						<p className="font-bold">
							{index}. {Object.keys(command)[0]}
						</p>
						{'MoveCall' in command && (
							<div className="flex flex-wrap gap-3">
								Package:{' '}
								<ReplayLink id={command.MoveCall.package} text={command.MoveCall.package} />
								Module:{' '}
								<ReplayLink
									id={command.MoveCall.package}
									module={command.MoveCall.module}
									text={command.MoveCall.module}
								/>
								Function: {command.MoveCall.function}
							</div>
						)}
					</PreviewCard.Header>

					<PreviewCard.Body>
						<div className="max-h-[300px] overflow-y-auto grid grid-cols-1 gap-2">
							{'MoveCall' in command &&
								command.MoveCall.arguments.map((argument) => renderArgument(argument))}

							{'SplitCoins' in command && (
								<>
									<div>Coin: {renderArgument(command.SplitCoins[0])}</div>
									<div>
										Amounts:
										{command.SplitCoins[1].map((argument) => renderArgument(argument))}
									</div>
								</>
							)}

							{'TransferObjects' in command && (
								<>
									<div>Address: {renderArgument(command.TransferObjects[1])}</div>
									<div>
										Objects:
										{command.TransferObjects[0].map((argument) => renderArgument(argument))}
									</div>
								</>
							)}

							{'MergeCoins' in command && (
								<>
									<div>Coin: {renderArgument(command.MergeCoins[0])}</div>
									<div>
										Coins:
										{command.MergeCoins[1].map((argument) => renderArgument(argument))}
									</div>
								</>
							)}
							{'MakeMoveVec' in command && (
								<>
									<div>
										Arguments:
										{command.MakeMoveVec[1].map((argument) => renderArgument(argument))}
									</div>
								</>
							)}
							{'Publish' in command && (
								<>
									<div>
										ObjectIds: {command.Publish[1].map((argument) => renderArgument(argument))}
									</div>
								</>
							)}
							{'Upgrade' in command && (
								<>
									<div>ObjectId: {renderArgument(command.Upgrade[2])}</div>
								</>
							)}
						</div>
					</PreviewCard.Body>
				</PreviewCard.Root>
			))}
		</div>
	);
}
