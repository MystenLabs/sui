// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Button } from '_app/shared/ButtonUI';
import { useZodForm } from '@mysten/core';
import { type SubmitHandler } from 'react-hook-form';
import { useNavigate } from 'react-router-dom';
import { z } from 'zod';

import { privateKeyValidation } from '../../helpers/validation/privateKeyValidation';
import { Form } from '../../shared/forms/Form';
import { TextAreaField } from '../../shared/forms/TextAreaField';
import Alert from '../alert';

const formSchema = z.object({
	privateKey: privateKeyValidation,
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
		watch,
	} = form;
	const navigate = useNavigate();
	const privateKey = watch('privateKey');
	const isHexadecimal = isValid && !privateKey.startsWith('suiprivkey');
	return (
		<Form className="flex flex-col h-full gap-2" form={form} onSubmit={onSubmit}>
			<TextAreaField label="Enter Private Key" rows={4} {...register('privateKey')} />
			{isHexadecimal ? (
				<Alert mode="warning">
					Importing Hex encoded Private Key will soon be deprecated, please use Bech32 encoded
					private key that starts with "suiprivkey" instead
				</Alert>
			) : null}
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
