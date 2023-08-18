// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useZodForm } from '@mysten/core';
import { type SubmitHandler } from 'react-hook-form';
import { useNavigate } from 'react-router-dom';
import { z } from 'zod';
import Overlay from '../../overlay';
import { Button } from '_app/shared/ButtonUI';
import { useNextMenuUrl } from '_components/menu/hooks';
import { CheckboxField } from '_src/ui/app/shared/forms/CheckboxField';
import { Form } from '_src/ui/app/shared/forms/Form';
import { SelectField } from '_src/ui/app/shared/forms/SelectField';
import { TextField } from '_src/ui/app/shared/forms/TextField';

const LOCK_INTERVALS = ['Hour', 'Minute', 'Second'];

const formSchema = z.object({
	password: z.string().nonempty('Required'),
	confirmedPassword: z.string().nonempty('Required'),
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
	cancelButtonText?: string;
	onSubmit: SubmitHandler<FormValues>;
	displayToS?: boolean;
};

export function ProtectAccountForm({ submitButtonText, onSubmit }: ProtectAccountFormProps) {
	const form = useZodForm({
		mode: 'all',
		schema: formSchema,
		defaultValues: {
			password: '',
			confirmedPassword: '',
			enabledAutolock: true,
			autoLockTimer: 1,
			autoLockInterval: 'Hour',
		},
	});
	const {
		register,
		formState: { isSubmitting, isValid },
	} = form;
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
				<div className="flex gap-2.5">
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

export function PasswordProtect() {
	const mainMenuUrl = useNextMenuUrl(true, '/');
	const navigate = useNavigate();
	return (
		<Overlay
			showModal={true}
			title={'Password Protect Accounts'}
			closeOverlay={() => navigate(mainMenuUrl)}
		>
			<div className="flex flex-col w-full mt-2.5">
				<ProtectAccountForm
					displayToS={false}
					submitButtonText="Save"
					onSubmit={(formValues) => {
						// eslint-disable-next-line no-console
						console.log(
							'TODO: Do something when the user submits the form successfully',
							formValues,
						);
					}}
				/>
			</div>
		</Overlay>
	);
}
