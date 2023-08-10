// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useZodForm } from '@mysten/core';
import { type SubmitHandler } from 'react-hook-form';
import { useNavigate } from 'react-router-dom';
import { z } from 'zod';
import { CheckboxField } from '../../shared/forms/CheckboxField';
import { Form } from '../../shared/forms/Form';
import { SelectField } from '../../shared/forms/SelectField';
import { TextField } from '../../shared/forms/TextField';
import ExternalLink from '../external-link';
import { Button } from '_app/shared/ButtonUI';
import { ToS_LINK } from '_src/shared/constants';

const LOCK_INTERVALS = ['Hour', 'Minute', 'Second'];

const formSchema = z.object({
	password: z.string().nonempty('Required'),
	confirmedPassword: z.string().nonempty('Required'),
	acceptedTos: z.literal<boolean>(true),
	enabledAutolock: z.boolean(),
	autoLockTimer: z.preprocess(
		(a) => parseInt(z.string().parse(a), 10),
		z.number().gte(0, 'Must be greater than 0'),
	),
	autoLockInterval: z.enum(['Hour', 'Minute', 'Second']),
});

type FormValues = z.infer<typeof formSchema>;

type ProtectAccountFormProps = {
	submitButtonText: string;
	cancelButtonText: string;
	onSubmit: SubmitHandler<FormValues>;
};

export function ProtectAccountForm({
	submitButtonText,
	cancelButtonText,
	onSubmit,
}: ProtectAccountFormProps) {
	const form = useZodForm({
		mode: 'all',
		schema: formSchema,
		defaultValues: {
			password: '',
			confirmedPassword: '',
			acceptedTos: false,
			enabledAutolock: true,
			autoLockTimer: 1,
			autoLockInterval: 'Hour',
		},
	});
	const {
		register,
		formState: { isSubmitting, isValid },
	} = form;
	const navigate = useNavigate();
	return (
		<Form className="flex flex-col gap-6 h-full" form={form} onSubmit={onSubmit}>
			<TextField
				autoFocus
				type="password"
				label="Create Account Password"
				{...register('password')}
			/>
			<TextField
				type="password"
				label="Confirm Account Password"
				{...register('confirmedPassword')}
			/>
			<div className="flex flex-col gap-4">
				<CheckboxField name="enabledAutolock" label="Auto-lock after I am inactive for" />
				<div className="flex items-start justify-between gap-2">
					<TextField type="number" {...register('autoLockTimer')} />
					<SelectField name="autoLockInterval" options={LOCK_INTERVALS} />
				</div>
			</div>
			<div className="flex flex-col gap-5 mt-auto">
				<CheckboxField
					name="acceptedTos"
					label={
						<>
							I read and agreed to the{' '}
							<ExternalLink href={ToS_LINK} className="text-[#1F6493] no-underline">
								Terms of Services
							</ExternalLink>
						</>
					}
				/>
				<div className="flex gap-2.5">
					<Button
						variant="outline"
						size="tall"
						text={cancelButtonText}
						onClick={() => navigate(-1)}
					/>
					<Button
						type="submit"
						disabled={isSubmitting || !isValid}
						variant="primary"
						size="tall"
						loading={isSubmitting}
						text={submitButtonText}
					/>
				</div>
			</div>
		</Form>
	);
}
