// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useZodForm } from '@mysten/core';
import { type ComponentProps, forwardRef } from 'react';
import toast from 'react-hot-toast';
import { z } from 'zod';
import { useAccounts } from '../../hooks/useAccounts';
import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { Form } from '../../shared/forms/Form';

type InputProps = Omit<ComponentProps<'input'>, 'className'>;

export const Input = forwardRef<HTMLInputElement, InputProps>((props, forwardedRef) => (
	<input
		className="transition peer items-center border-none outline-none bg-transparent hover:text-hero rounded-sm text-pBody text-steel-darker font-semibold p-0 focus:bg-transparent"
		ref={forwardedRef}
		{...props}
		id="current-address-nickname-edit"
	/>
));

const formSchema = z.object({
	nickname: z.string().trim(),
});

export function EditableAccountName({ accountID, name }: { accountID: string; name: string }) {
	const backgroundClient = useBackgroundClient();
	const { data: accounts } = useAccounts();
	const account = accounts?.find((account) => account.id === accountID);
	const form = useZodForm({
		mode: 'all',
		schema: formSchema,
		defaultValues: {
			nickname: name,
		},
	});
	const { register } = form;

	const onSubmit = async ({ nickname }: { nickname: string }) => {
		if (account && accountID) {
			try {
				await backgroundClient.setAccountNickname({
					id: accountID,
					nickname: nickname || null,
				});
			} catch (e) {
				toast.error((e as Error).message || 'Failed to set nickname');
			}
		}
	};

	const handleKeyPress = (e: React.KeyboardEvent<HTMLFormElement>) => {
		if (e.key === 'Enter') {
			e.preventDefault();
			form.handleSubmit(onSubmit)();
			const inputElement = document.getElementById('current-address-nickname-edit');
			inputElement?.blur();
		}
	};

	return (
		<div>
			<Form className="flex flex-col" form={form} onSubmit={onSubmit} onKeyPress={handleKeyPress}>
				<Input {...register('nickname')} onBlur={() => form.handleSubmit(onSubmit)()} />
			</Form>
		</div>
	);
}
