// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowBgFill16, Plus12 } from '@mysten/icons';
import * as CollapsiblePrimitive from '@radix-ui/react-collapsible';
import { type AccountType, type SerializedUIAccount } from '_src/background/accounts/Account';
import { AccountItem } from '_src/ui/app/components/accounts/AccountItem';
import { AccountActions } from '_src/ui/app/components/menu/content/AccountActions';
import { Heading } from '_src/ui/app/shared/heading';
import { Text } from '_src/ui/app/shared/text';
import { ButtonOrLink, type ButtonOrLinkProps } from '_src/ui/app/shared/utils/ButtonOrLink';

// todo: these will likely be shared in various parts of the UI
const labels: Record<AccountType, string> = {
	'mnemonic-derived': 'Passphrase Derived',
	ledger: 'Ledger',
	imported: 'Imported',
	qredo: 'Qredo',
	zk: 'zkLogin',
};

// todo: we probbaly have some duplication here with the various FooterLink / ButtonOrLink
// components - we should look to add these to base components somewhere
function FooterLink({ children, to, ...props }: ButtonOrLinkProps) {
	return (
		<ButtonOrLink
			className="text-hero-darkest/40 no-underline uppercase group-hover:text-hero"
			to={to}
			{...props}
		>
			<Text variant="captionSmallExtra" weight="medium">
				{children}
			</Text>
		</ButtonOrLink>
	);
}

// todo: this is slightly different than the account footer in the AccountsList - look to consolidate :(
function AccountFooter({ accountID }: { accountID: string }) {
	return (
		<div className="flex flex-shrink-0 w-full">
			<div className="flex gap-3">
				<div className="w-4" />
				<FooterLink to={`/accounts/edit/${accountID}`}>Edit Nickname</FooterLink>
				<FooterLink to="/remove">Remove</FooterLink>
			</div>
		</div>
	);
}

export function AccountGroup({
	accounts,
	type,
}: {
	accounts: SerializedUIAccount[];
	type: AccountType;
}) {
	return (
		<CollapsiblePrimitive.Root defaultOpen={true} asChild>
			<div className="flex flex-col gap-4 h-full w-full ">
				<CollapsiblePrimitive.Trigger asChild>
					<div className="flex gap-2 w-full items-center justify-center cursor-pointer flex-shrink-0 group [&>*]:select-none">
						<ArrowBgFill16 className="h-4 w-4 group-data-[state=open]:rotate-90 text-hero-darkest/20" />
						<Heading variant="heading5" weight="semibold" color="steel-darker">
							{labels[type]}
						</Heading>
						<div className="h-px bg-gray-45 flex flex-1 flex-shrink-0" />
						<ButtonOrLink
							onClick={(e) => {
								// prevent button click from collapsing section
								e.stopPropagation();
								// todo: implement me
							}}
							className="items-center justify-center gap-0.5 cursor-pointer appearance-none uppercase flex bg-transparent border-0 outline-none text-hero hover:text-hero-darkest"
						>
							<Plus12 />
							<Text variant="bodySmall" weight="semibold">
								New
							</Text>
						</ButtonOrLink>
					</div>
				</CollapsiblePrimitive.Trigger>
				<CollapsiblePrimitive.CollapsibleContent asChild>
					<div className="flex flex-col gap-3 w-full flex-shrink-0">
						{accounts.map((account) => {
							return (
								<AccountItem
									key={account.id}
									background="gradient"
									address={account.address}
									after={<AccountFooter accountID={account.id} />}
								/>
							);
						})}
						<AccountActions account={accounts[0]} />
					</div>
				</CollapsiblePrimitive.CollapsibleContent>
			</div>
		</CollapsiblePrimitive.Root>
	);
}
