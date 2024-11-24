// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useResolveSuiNSName } from '_app/hooks/useAppResolveSuinsName';
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
	DialogTrigger,
} from '_src/ui/app/shared/Dialog';
import { useZodForm } from '@mysten/core';
import { useEffect, useState } from 'react';
import toast from 'react-hot-toast';
import { z } from 'zod';

import { useAccounts } from '../../hooks/useAccounts';
import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { Button } from '../../shared/ButtonUI';
import { Form } from '../../shared/forms/Form';
import { TextField } from '../../shared/forms/TextField';

const formSchema = z.object({
	nickname: z.string().trim(),
	useDomain: z.boolean().optional(),
});

interface NicknameDialogProps {
	accountID: string;
	trigger: JSX.Element;
}

type FormValues = {
	nickname: string;
	useDomain?: boolean;
};

export function NicknameDialog({ accountID, trigger }: NicknameDialogProps) {
	const [open, setOpen] = useState(false);
	const backgroundClient = useBackgroundClient();
	const { data: accounts } = useAccounts();
	const account = accounts?.find((account) => account.id === accountID);
	const domainName = useResolveSuiNSName(account?.address);

	const form = useZodForm({
		mode: 'all',
		schema: formSchema,
		defaultValues: {
			nickname: account?.nickname ?? '',
			useDomain: false,
		},
	});

	const {
		register,
		watch,
		setValue,
		formState: { isSubmitting, isValid },
	} = form;

	const useDomain = watch('useDomain');

	useEffect(() => {
		if (useDomain) {
			setValue('nickname', '');
		} else {
			setValue('nickname', account?.nickname ?? '');
		}
	}, [useDomain, account?.nickname, setValue]);

	const onSubmit = async ({ nickname, useDomain }: FormValues) => {
		if (account && accountID) {
			try {
				const finalNickname = useDomain ? null : nickname;
				await backgroundClient.setAccountNickname({
					id: accountID,
					nickname: finalNickname,
				});
				setOpen(false);
			} catch (e) {
				toast.error((e as Error).message || 'Failed to set nickname');
			}
		}
	};

	return (
		<Dialog open={open} onOpenChange={setOpen}>
			<DialogTrigger asChild>{trigger}</DialogTrigger>
			<DialogContent onPointerDownOutside={(e: Event) => e.preventDefault()}>
				<DialogHeader>
					<DialogTitle>Account Nickname</DialogTitle>
					<DialogDescription asChild>
						<span className="sr-only">Enter your account password to unlock your account</span>
					</DialogDescription>
				</DialogHeader>
				<Form className="flex flex-col gap-6 h-full" form={form} onSubmit={onSubmit}>
					{domainName && (
						<div className="flex items-center gap-2">
							<input type="checkbox" {...register('useDomain')} id="use-domain" />
							<label htmlFor="use-domain" className="text-sm flex items-center gap-1">
							Use Sui Name Service:
								<span className="font-medium text-hero px-2 py-0.5 bg-hero/10 rounded-lg">
									{domainName}
								</span>
							</label>
						</div>
					)}
					<TextField
						label="Personalize account with a nickname."
						{...register('nickname')}
						disabled={useDomain}
					/>
					<div className="flex gap-2.5">
						<Button variant="outline" size="tall" text="Cancel" onClick={() => setOpen(false)} />
						<Button
							type="submit"
							disabled={isSubmitting || !isValid}
							variant="primary"
							size="tall"
							loading={isSubmitting}
							text="Save"
						/>
					</div>
				</Form>
			</DialogContent>
		</Dialog>
	);
}