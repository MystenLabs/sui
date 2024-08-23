// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	ConnectModal,
	useAccounts,
	useCurrentAccount,
	useDisconnectWallet,
	useSwitchAccount,
} from '@mysten/dapp-kit';
import { formatAddress } from '@mysten/sui/utils';
import { Check, ChevronsUpDown } from 'lucide-react';
import { useState } from 'react';

import { cn } from '@/lib/utils';

import { Button } from './ui/button';
import {
	Command,
	CommandEmpty,
	CommandGroup,
	CommandInput,
	CommandItem,
	CommandList,
} from './ui/command';
import { Popover, PopoverContent, PopoverTrigger } from './ui/popover';

function ConnectedButton() {
	const accounts = useAccounts();
	const currentAccount = useCurrentAccount();
	const { mutateAsync: switchAccount } = useSwitchAccount();
	const { mutateAsync: disconnect } = useDisconnectWallet();
	const [open, setOpen] = useState(false);

	return (
		<Popover open={open} onOpenChange={setOpen}>
			<PopoverTrigger asChild>
				<Button
					variant="outline"
					role="combobox"
					aria-expanded={open}
					className="w-[180px] justify-between"
				>
					{currentAccount ? formatAddress(currentAccount.address) : '...'}
					<ChevronsUpDown className="ml-2 h-4 w-4 shrink-0 opacity-50" />
				</Button>
			</PopoverTrigger>
			<PopoverContent className="w-[180px] p-0">
				<Command>
					<CommandInput placeholder="Search accounts..." />
					<CommandList>
						<CommandEmpty>No account found.</CommandEmpty>
						<CommandGroup>
							{accounts.map((account) => (
								<CommandItem
									key={account.address}
									value={account.address}
									className="cursor-pointer"
									onSelect={() => {
										switchAccount({ account });
										setOpen(false);
									}}
								>
									<Check
										className={cn(
											'mr-2 h-4 w-4',
											currentAccount?.address === account.address ? 'opacity-100' : 'opacity-0',
										)}
									/>
									{formatAddress(account.address)}
								</CommandItem>
							))}

							<CommandItem
								className="cursor-pointer"
								onSelect={() => {
									disconnect();
								}}
							>
								Disconnect
							</CommandItem>
						</CommandGroup>
					</CommandList>
				</Command>
			</PopoverContent>
		</Popover>
	);
}

export function ConnectWallet() {
	const [connectModalOpen, setConnectModalOpen] = useState(false);
	const currentAccount = useCurrentAccount();

	return (
		<>
			{currentAccount ? (
				<ConnectedButton />
			) : (
				<Button onClick={() => setConnectModalOpen(true)}>Connect Wallet</Button>
			)}

			<ConnectModal
				trigger={<></>}
				open={connectModalOpen}
				onOpenChange={(open) => setConnectModalOpen(open)}
			/>
		</>
	);
}
