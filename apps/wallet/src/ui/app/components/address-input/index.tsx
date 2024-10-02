// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '_app/shared/text';
import Alert from '_src/ui/app/components/alert';
import { useSuiClient } from '@mysten/dapp-kit';
import { QrCode, X12 } from '@mysten/icons';
import { isValidSuiAddress } from '@mysten/sui/utils';
import { useQuery } from '@tanstack/react-query';
import { cx } from 'class-variance-authority';
import { useField, useFormikContext } from 'formik';
import { useCallback, useMemo } from 'react';
import type { ChangeEventHandler } from 'react';
import TextareaAutosize from 'react-textarea-autosize';

import { useSuiAddressValidation } from './validation';

export interface AddressInputProps {
	disabled?: boolean;
	placeholder?: string;
	name: string;
}

enum RecipientWarningType {
	OBJECT = 'OBJECT',
	EMPTY = 'EMPTY',
}

export function AddressInput({
	disabled: forcedDisabled,
	placeholder = '0x...',
	name = 'to',
}: AddressInputProps) {
	const [field, meta] = useField(name);

	const client = useSuiClient();
	const { data: warningData } = useQuery({
		queryKey: ['address-input-warning', field.value],
		queryFn: async () => {
			// We assume this validation will happen elsewhere:
			if (!isValidSuiAddress(field.value)) {
				return null;
			}

			const object = await client.getObject({ id: field.value });

			if (object && 'data' in object) {
				return RecipientWarningType.OBJECT;
			}

			const [fromAddr, toAddr] = await Promise.all([
				client.queryTransactionBlocks({
					filter: { FromAddress: field.value },
					limit: 1,
				}),
				client.queryTransactionBlocks({
					filter: { ToAddress: field.value },
					limit: 1,
				}),
			]);

			if (fromAddr.data?.length === 0 && toAddr.data?.length === 0) {
				return RecipientWarningType.EMPTY;
			}

			return null;
		},
		enabled: !!field.value,
		gcTime: 10 * 1000,
		refetchOnMount: false,
		refetchOnWindowFocus: false,
		refetchInterval: false,
	});

	const { isSubmitting, setFieldValue, isValidating } = useFormikContext();
	const suiAddressValidation = useSuiAddressValidation();

	const disabled = forcedDisabled !== undefined ? forcedDisabled : isSubmitting;
	const handleOnChange = useCallback<ChangeEventHandler<HTMLTextAreaElement>>(
		(e) => {
			const address = e.currentTarget.value;
			setFieldValue(name, suiAddressValidation.cast(address));
		},
		[setFieldValue, name, suiAddressValidation],
	);
	const formattedValue = useMemo(
		() => suiAddressValidation.cast(field?.value),
		[field?.value, suiAddressValidation],
	);

	const clearAddress = useCallback(() => {
		setFieldValue('to', '');
	}, [setFieldValue]);

	const hasWarningOrError = meta.touched && (meta.error || warningData) && !isValidating;

	return (
		<>
			<div
				className={cx(
					'flex h-max w-full rounded-2lg bg-white border border-solid box-border focus-within:border-steel transition-all overflow-hidden',
					hasWarningOrError ? 'border-issue' : 'border-gray-45',
				)}
			>
				<div className="min-h-[42px] w-full flex items-center pl-3 py-2">
					<TextareaAutosize
						data-testid="address-input"
						maxRows={3}
						minRows={1}
						disabled={disabled}
						placeholder={placeholder}
						value={formattedValue}
						onChange={handleOnChange}
						onBlur={field.onBlur}
						className={cx(
							'w-full text-bodySmall leading-100 font-medium font-mono bg-white placeholder:text-steel-dark placeholder:font-normal placeholder:font-mono border-none resize-none',
							hasWarningOrError ? 'text-issue' : 'text-gray-90',
						)}
						name={name}
					/>
				</div>

				<div
					onClick={clearAddress}
					className="flex bg-gray-40 items-center justify-center w-11 right-0 max-w-[20%] ml-4 cursor-pointer"
				>
					{meta.touched && field.value ? (
						<X12 className="h-3 w-3 text-steel-darker" />
					) : (
						<QrCode className="h-5 w-5 text-steel-darker" />
					)}
				</div>
			</div>

			{field.value && !isValidating ? (
				<div className="mt-2.5 w-full">
					<Alert noBorder rounded="lg" mode={meta.error || warningData ? 'issue' : 'success'}>
						{warningData === RecipientWarningType.OBJECT ? (
							<>
								<Text variant="pBody" weight="semibold">
									This address is an Object
								</Text>
								<Text variant="pBodySmall" weight="medium">
									Once sent, the funds cannot be recovered. Please make sure you want to send coins
									to this address.
								</Text>
							</>
						) : warningData === RecipientWarningType.EMPTY ? (
							<>
								<Text variant="pBody" weight="semibold">
									This address has no prior transactions
								</Text>
								<Text variant="pBodySmall" weight="medium">
									Please make sure you want to send coins to this address.
								</Text>
							</>
						) : (
							<Text variant="pBodySmall" weight="medium">
								{meta.error || 'Valid address'}
							</Text>
						)}
					</Alert>
				</div>
			) : null}
		</>
	);
}
