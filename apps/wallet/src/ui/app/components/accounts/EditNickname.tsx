// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useZodForm } from '@mysten/core';
import toast from 'react-hot-toast';
import { useNavigate, useParams } from 'react-router-dom';
import { z } from 'zod';
import { useAccounts } from '../../hooks/useAccounts';
import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { Button } from '../../shared/ButtonUI';
import { Form } from '../../shared/forms/Form';
import { TextField } from '../../shared/forms/TextField';
import {
	Dialog,
	DialogContent,
	DialogHeader,
	DialogTitle,
	DialogDescription,
} from '_src/ui/app/shared/Dialog';

const formSchema = z.object({
	nickname: z.string().trim(),
});

export function EditNickname() {
	const { accountID } = useParams();
	const navigate = useNavigate();
	const backgroundClient = useBackgroundClient();
	const { data: accounts } = useAccounts();
	const account = accounts?.find((account) => account.id === accountID);

	const form = useZodForm({
		mode: 'all',
		schema: formSchema,
		defaultValues: {
			nickname: account?.nickname ?? '',
		},
	});
	const {
		register,
		formState: { isSubmitting, isValid },
	} = form;

	const close = () => navigate('/accounts/manage');
	const onSubmit = async ({ nickname }: { nickname: string }) => {
		if (account && accountID) {
			try {
				await backgroundClient.setAccountNickname({
					id: accountID,
					nickname: nickname || null,
				});
				close();
			} catch (e) {
				toast.error((e as Error).message || 'Failed to set nickname');
			}
		}
	};

	return (
		<Dialog defaultOpen>
			<DialogContent onPointerDownOutside={(e: Event) => e.preventDefault()}>
				<DialogHeader>
					<DialogTitle>Account Nickname</DialogTitle>
					<DialogDescription asChild>
						<span className="sr-only">Enter your account password to unlock your account</span>
					</DialogDescription>
				</DialogHeader>
				<Form className="flex flex-col gap-6 h-full" form={form} onSubmit={onSubmit}>
					<TextField label="Personalize account with a nickname." {...register('nickname')} />
					<div className="flex gap-2.5">
						<Button variant="outline" size="tall" text="Cancel" onClick={close} />
						<Button
							type="submit"
							disabled={isSubmitting || !isValid}
							variant="primary"
							size="tall"
							loading={isSubmitting}
							text={'Save'}
						/>
					</div>
				</Form>
			</DialogContent>
		</Dialog>
	);
}
