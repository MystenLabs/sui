// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { yupResolver } from '@hookform/resolvers/yup';
import { useForm, type SubmitHandler } from 'react-hook-form';
import { useNavigate } from 'react-router-dom';
import * as Yup from 'yup';
import { privateKeyValidation } from '../../helpers/validation/privateKeyValidation';
import { Button } from '_app/shared/ButtonUI';
import Alert from '_src/ui/app/components/alert';
import { Text } from '_src/ui/app/shared/text';

const formSchema = Yup.object({
	privateKey: privateKeyValidation,
});

type FormValues = Yup.InferType<typeof formSchema>;

type ImportPrivateKeyFormProps = {
	onSubmit: SubmitHandler<FormValues>;
};

export function ImportPrivateKeyForm({ onSubmit }: ImportPrivateKeyFormProps) {
	const {
		register,
		handleSubmit,
		formState: { isSubmitting, isValid, touchedFields, errors },
	} = useForm({
		mode: 'onTouched',
		resolver: yupResolver(formSchema),
	});
	const navigate = useNavigate();

	return (
		<form className="flex flex-col h-full" onSubmit={handleSubmit(onSubmit)}>
			<label className="flex flex-col gap-2.5">
				<div className="pl-2.5">
					<Text variant="pBody" color="steel-darker" weight="semibold">
						Enter Private Key
					</Text>
				</div>
				<textarea
					className="resize-none w-full text-body text-steel-dark font-medium p-3 border border-solid border-gray-45 rounded-2lg shadow-button focus:border-steel focus:shadow-none"
					rows={4}
					{...register('privateKey')}
				/>
				{touchedFields.privateKey && errors.privateKey && (
					<Alert>{errors.privateKey.message}</Alert>
				)}
			</label>
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
		</form>
	);
}
