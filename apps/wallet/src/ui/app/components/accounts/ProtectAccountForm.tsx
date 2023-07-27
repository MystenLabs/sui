// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { yupResolver } from '@hookform/resolvers/yup';
import { useForm, type SubmitHandler } from 'react-hook-form';
import { useNavigate } from 'react-router-dom';
import * as Yup from 'yup';
import { CheckboxField } from '../../shared/forms/CheckboxField';
import { Form } from '../../shared/forms/Form';
import { TextField } from '../../shared/forms/TextField';
import ExternalLink from '../external-link';
import { Button } from '_app/shared/ButtonUI';
import { ToS_LINK } from '_src/shared/constants';

const formSchema = Yup.object({
	password: Yup.string().required('Required'),
	confirmedPassword: Yup.string().required('Required'),
	acceptedTos: Yup.boolean().required().oneOf([true]),
	enabledAutolock: Yup.boolean(),
});

type FormValues = Yup.InferType<typeof formSchema>;

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
	const form = useForm({
		mode: 'all',
		defaultValues: {
			password: '',
			confirmedPassword: '',
			acceptedTos: false,
			enabledAutolock: true,
		},
		resolver: yupResolver(formSchema),
	});
	const {
		register,
		formState: { isSubmitting, isValid },
	} = form;
	const navigate = useNavigate();

	return (
		<Form className="flex flex-col gap-6 h-full" form={form} onSubmit={onSubmit}>
			<TextField type="password" label="Create Account Password" {...register('password')} />
			<TextField
				type="password"
				label="Confirm Account Password"
				{...register('confirmedPassword')}
			/>
			<div className="flex flex-col gap-4">
				<CheckboxField name="enabledAutolock" label="Auto-lock after I am inactive for" />
				{/* TODO: Abhi is working on designs for the auto-lock input, we'll add this when it's ready */}
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
