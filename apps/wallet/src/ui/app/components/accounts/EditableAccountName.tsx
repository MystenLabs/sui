// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useZodForm } from '@mysten/core';
import { type ComponentProps, forwardRef, useRef } from 'react';
import toast from 'react-hot-toast';
import { z } from 'zod';
import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { Form } from '../../shared/forms/Form';

type InputProps = Omit<ComponentProps<'input'>, 'className'>;

export const Input = forwardRef<HTMLInputElement, InputProps>((props, forwardedRef) => (
	<input
		className="transition peer items-center border-none outline-none bg-transparent hover:text-hero rounded-sm text-pBody text-steel-darker font-semibold p-0 focus:bg-transparent"
		ref={forwardedRef}
		{...props}
	/>
));

const formSchema = z.object({
	nickname: z.string().trim(),
});

export function EditableAccountName({ accountID, name }: { accountID: string; name: string }) {
	const backgroundClient = useBackgroundClient();
	const form = useZodForm({
		mode: 'all',
		schema: formSchema,
		defaultValues: {
			nickname: name,
		},
	});
	const { register } = form;
	const { ref, ...rest } = register('nickname');
	const inputRef = useRef<HTMLInputElement | null>(null);

	const onSubmit = async ({ nickname }: { nickname: string }) => {
		if (accountID) {
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

	const handleKeyDown = (e: React.KeyboardEvent<HTMLFormElement>) => {
		if (e.key === 'Enter') {
			e.preventDefault();
			form.handleSubmit(onSubmit)();
			inputRef.current?.blur();
		}
	};

	return (
		<div>
			<Form className="flex flex-col" form={form} onSubmit={onSubmit} onKeyDown={handleKeyDown}>
				<Input
					{...rest}
					onBlur={() => form.handleSubmit(onSubmit)()}
					ref={(e) => {
						ref(e);
						inputRef.current = e;
					}}
				/>
			</Form>
		</div>
	);
}
