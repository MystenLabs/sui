// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useZodForm } from '@mysten/core';
import { hexToBytes } from '@noble/hashes/utils';
import { type SubmitHandler } from 'react-hook-form';
import { useNavigate } from 'react-router-dom';
import { z } from 'zod';
import { Form } from '../../shared/forms/Form';
import { TextAreaField } from '../../shared/forms/TextAreaField';
import { Button } from '_app/shared/ButtonUI';

const formSchema = z.object({
	privateKey: z
		.string()
		.trim()
		.transform((privateKey, context) => {
			const hexValue = privateKey.startsWith('0x') ? privateKey.slice(2) : privateKey;
			let privateKeyBytes: Uint8Array | undefined;

			try {
				privateKeyBytes = hexToBytes(hexValue);
			} catch (error) {
				context.addIssue({
					code: 'custom',
					message: 'Private Key must be a hexadecimal value. It may optionally begin with "0x".',
				});
				return z.NEVER;
			}

			if ([32, 64].includes(privateKeyBytes.length)) {
				context.addIssue({
					code: 'custom',
					message: 'Private Key must be either 32 or 64 bytes.',
				});
				return z.NEVER;
			}
			return hexValue;
		}),
});

type FormValues = z.infer<typeof formSchema>;

type ImportPrivateKeyFormProps = {
	onSubmit: SubmitHandler<FormValues>;
};

export function ImportPrivateKeyForm({ onSubmit }: ImportPrivateKeyFormProps) {
	const form = useZodForm({
		mode: 'onTouched',
		schema: formSchema,
	});
	const {
		register,
		formState: { isSubmitting, isValid },
	} = form;
	const navigate = useNavigate();

	return (
		<Form className="flex flex-col h-full" form={form} onSubmit={onSubmit}>
			<TextAreaField label="Enter Private Key" rows={4} {...register('privateKey')} />
			<div className="flex gap-2.5 mt-auto">
				<Button variant="outline" size="tall" text="Cancel" onClick={() => navigate(-1)} />
				<Button
					type="submit"
					disabled={isSubmitting || !isValid}
					variant="primary"
					size="tall"
					loading={isSubmitting}
					text="Add Account"
				/>
			</div>
		</Form>
	);
}
