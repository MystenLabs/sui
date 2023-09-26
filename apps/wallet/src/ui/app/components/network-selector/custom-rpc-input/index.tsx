// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useAppDispatch, useAppSelector } from '_hooks';
import { changeActiveNetwork } from '_redux/slices/app';
import { ampli } from '_src/shared/analytics/ampli';
import { API_ENV } from '_src/shared/api-env';
import { isValidUrl } from '_src/shared/utils';
import { InputWithAction } from '_src/ui/app/shared/InputWithAction';
import { Form, Formik } from 'formik';
import { useCallback } from 'react';
import { toast } from 'react-hot-toast';
import * as Yup from 'yup';

const MIN_CHAR = 5;

const validation = Yup.object({
	rpcInput: Yup.string()
		.required()
		.label('Custom RPC URL')
		.min(MIN_CHAR)
		.test('validate-url', 'Not a valid URL', (value) => isValidUrl(value || null)),
});

export function CustomRPCInput() {
	const placeholder = 'http://localhost:3000/';

	const customRPC = useAppSelector(({ app }) => app.customRPC || '');

	const dispatch = useAppDispatch();

	const changeNetwork = useCallback(
		async ({ rpcInput }: { rpcInput: string }) => {
			try {
				await dispatch(
					changeActiveNetwork({
						network: {
							env: API_ENV.customRPC,
							customRpcUrl: rpcInput,
						},
						store: true,
					}),
				).unwrap();
				ampli.switchedNetwork({
					toNetwork: rpcInput,
				});
			} catch (e) {
				toast.error((e as Error).message);
			}
		},
		[dispatch],
	);

	return (
		<Formik
			initialValues={{ rpcInput: customRPC }}
			validationSchema={validation}
			onSubmit={changeNetwork}
			enableReinitialize={true}
		>
			<Form>
				<InputWithAction
					type="text"
					name="rpcInput"
					min={MIN_CHAR}
					actionDisabled="auto"
					placeholder={placeholder}
					actionText="Save"
				/>
			</Form>
		</Formik>
	);
}
