// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useZodForm } from '@mysten/core';
import { useNavigate, useParams } from 'react-router-dom';
import { z } from 'zod';
import { useAccountNicknames } from './NicknamesProvider';
import { Button } from '../../shared/ButtonUI';
import { Form } from '../../shared/forms/Form';
import { TextField } from '../../shared/forms/TextField';
import Overlay from '../overlay';

const formSchema = z.object({
	nickname: z.string().nonempty('Required'),
});

export function EditNickname() {
	const { address } = useParams();
	const navigate = useNavigate();
	const { setAccountNickname, accountNicknames } = useAccountNicknames();

	const form = useZodForm({
		mode: 'all',
		schema: formSchema,
		defaultValues: {
			nickname: '',
		},
	});
	const {
		register,
		formState: { isSubmitting, isValid },
	} = form;

	const onSubmit = ({ nickname }: { nickname: string }) => {
		address && setAccountNickname(address, nickname);
	};
	return (
		<Overlay showModal closeOverlay={() => navigate('/accounts/manage')}>
			<Form className="flex flex-col gap-6 h-full" form={form} onSubmit={onSubmit}>
				<div>
					<TextField label="Enter account nickname" {...register('nickname')} />
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
		</Overlay>
	);
}
