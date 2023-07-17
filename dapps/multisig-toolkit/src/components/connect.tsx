// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Button } from './ui/button';
import { useState } from 'react';
import { useWalletKit, ConnectModal } from '@mysten/wallet-kit';
import { Popover, PopoverContent, PopoverTrigger } from './ui/popover';
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem } from './ui/command';
import { formatAddress } from '@mysten/sui.js';
import { Check, ChevronsUpDown } from 'lucide-react';
import { cn } from '@/lib/utils';

function ConnectedButton() {
	const { accounts, currentAccount, selectAccount, disconnect } = useWalletKit();
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
					<CommandEmpty>No account found.</CommandEmpty>
					<CommandGroup>
						{accounts.map((account) => (
							<CommandItem
								key={account.address}
								value={account.address}
								className="cursor-pointer"
								onSelect={() => {
									selectAccount(account);
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
							Logout
						</CommandItem>
					</CommandGroup>
				</Command>
			</PopoverContent>
		</Popover>
	);
}

export function ConnectWallet() {
	const [connectModalOpen, setConnectModalOpen] = useState(false);
	const { currentAccount } = useWalletKit();

	return (
		<>
			{currentAccount ? (
				<ConnectedButton />
			) : (
				<Button onClick={() => setConnectModalOpen(true)}>Connect Wallet</Button>
			)}

			<ConnectModal open={connectModalOpen} onClose={() => setConnectModalOpen(false)} />
		</>
	);
}
