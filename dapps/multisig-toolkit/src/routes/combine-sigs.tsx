// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { PublicKey, SerializedSignature } from '@mysten/sui.js/cryptography';
import { MultiSigPublicKey, publicKeyFromSuiBytes } from '@mysten/sui.js/multisig';
import { useState } from 'react';
import { FieldValues, useFieldArray, useForm } from 'react-hook-form';

import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';

export default function MultiSigCombineSignatureGenerator() {
	const [msAddress, setMSAddress] = useState('');
	const [msSignature, setMSSignature] = useState('');
	const { register, control, handleSubmit } = useForm({
		defaultValues: {
			pubKeys: [{ pubKey: 'Sui Pubkey', weight: '', signature: 'Sui Signature' }],
			threshold: 1,
		},
	});
	const { fields, append, remove } = useFieldArray({
		control,
		name: 'pubKeys',
	});

	// Perform generation of multisig address
	const onSubmit = (data: FieldValues) => {
		// handle MultiSig Pubkeys, Weights, and Threshold
		let pks: { publicKey: PublicKey; weight: number }[] = [];
		let sigs: SerializedSignature[] = [];
		data.pubKeys.forEach((item: any) => {
			const pk = publicKeyFromSuiBytes(item.pubKey);
			pks.push({ publicKey: pk, weight: item.weight });
			if (item.signature) {
				sigs.push(item.signature);
			}
		});
		const multiSigPublicKey = MultiSigPublicKey.fromPublicKeys({
			threshold: data.threshold,
			publicKeys: pks,
		});
		const multisigSuiAddress = multiSigPublicKey.toSuiAddress();
		setMSAddress(multisigSuiAddress);
		const multisigCombinedSig = multiSigPublicKey.combinePartialSignatures(sigs);
		setMSSignature(multisigCombinedSig);
	};

	return (
		<div className="flex flex-col gap-4">
			<h2 className="scroll-m-20 text-4xl font-extrabold tracking-tight lg:text-5xl">
				MultiSig Combined Signature Creator
			</h2>

			<form className="flex flex-col gap-4" onSubmit={handleSubmit(onSubmit)}>
				<p>The following demo allow you to create Sui MultiSig Combined Signatures.</p>
				<p>Sui Pubkeys, weights, signatures for testing/playing with:</p>
				<div className="flex flex-col gap-2 bg-gray-600 p-4 rounded-md">
					<div className="flex gap-0 border-b">
						<div className="flex-1 font-bold border-r p-2">Sui Pubkeys</div>
						<div className="flex-1 font-bold border-r p-2">Weights</div>
						<div className="flex-1 font-bold p-2">Signatures</div>
					</div>
					<div className="flex gap-0 border-b">
						<div className="flex-1 break-all border-r p-2">
							ACaY7TW0MnPu+fr/Z2qH5YRybHsj80qfwfqiuduT4czi
						</div>
						<div className="flex-1 border-r p-2">1</div>
						<div className="flex-1 break-all p-2">
							AIYbCXAhPmILpWq6xsEY/Nu310Kednlb60Qcd/nD+u2WCXE/FvSXNRUQW9OQKGqt2CeskPyv2SEhaKMZ8gLkdQ8mmO01tDJz7vn6/2dqh+WEcmx7I/NKn8H6ornbk+HM4g==
						</div>
					</div>
					<div className="flex gap-0 border-b">
						<div className="flex-1 break-all border-r p-2">
							ABr818VXt+6PLPRoA7QnsHBfRpKJdWZPjt7ppiTl6Fkq
						</div>
						<div className="flex-1 border-r p-2">1</div>
						<div className="flex-1 p-2"></div>
					</div>
					<div className="flex gap-0">
						<div className="flex-1 break-all border-r p-2">
							ALDE3sq5JZOj3Hmo/UeUv14zi4TFQMFq/xCTaSH+swMS
						</div>
						<div className="flex-1 border-r p-2">1</div>
						<div className="flex-1 p-2"></div>
					</div>
				</div>
				<ul className="grid w-full gap-1.5">
					{fields.map((item, index) => {
						return (
							<li key={item.id}>
								<input
									className="min-h-[80px] rounded-md border border-input bg-transparent px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
									{...register(`pubKeys.${index}.pubKey`, { required: true })}
								/>

								<input
									className="min-h-[80px] rounded-md border border-input bg-transparent px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
									type="number"
									{...register(`pubKeys.${index}.weight`, { required: true })}
								/>
								<input
									className="min-h-[80px] rounded-md border border-input bg-transparent px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
									{...register(`pubKeys.${index}.signature`, { required: false })}
								/>

								<Button
									className="min-h-[80px] rounded-md border border-input px-3 py-2 text-sm padding-2"
									type="button"
									onClick={() => remove(index)}
								>
									Delete
								</Button>
							</li>
						);
					})}
				</ul>
				<section>
					<Button
						type="button"
						onClick={() => {
							append({ pubKey: 'Sui Pubkey', weight: '', signature: 'Sui Signature' });
						}}
					>
						New PubKey
					</Button>
				</section>
				<section>
					<label className="form-label min-h-[80px] rounded-md border text-sm px-3 py-2 ring-offset-background">
						MultiSig Threshold Value:
					</label>
					<input
						className="min-h-[80px] rounded-md border border-input bg-transparent px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
						type="number"
						{...register(`threshold`, { valueAsNumber: true, required: true })}
					/>
				</section>

				<Button type="submit">Submit</Button>
			</form>
			{msAddress && (
				<Card key={msAddress}>
					<CardHeader>
						<CardTitle>Sui MultiSig Address</CardTitle>
						<CardDescription>
							https://docs.sui.io/testnet/learn/cryptography/sui-multisig
						</CardDescription>
					</CardHeader>
					<CardContent>
						<div className="flex flex-col gap-2">
							<div className="bg-muted rounded text-sm font-mono p-2 break-all">{msAddress}</div>
						</div>
					</CardContent>
				</Card>
			)}
			{msSignature && (
				<Card key={msSignature}>
					<CardHeader>
						<CardTitle>Sui MultiSig Combined Address</CardTitle>
						<CardDescription>
							https://docs.sui.io/testnet/learn/cryptography/sui-multisig
						</CardDescription>
					</CardHeader>
					<CardContent>
						<div className="flex flex-col gap-2">
							<div className="bg-muted rounded text-sm font-mono p-2 break-all">{msSignature}</div>
						</div>
					</CardContent>
				</Card>
			)}
		</div>
	);
}

/*
sui keytool multi-sig-combine-partial-sig \
--pks \
ACaY7TW0MnPu+fr/Z2qH5YRybHsj80qfwfqiuduT4czi \
ABr818VXt+6PLPRoA7QnsHBfRpKJdWZPjt7ppiTl6Fkq \
ALDE3sq5JZOj3Hmo/UeUv14zi4TFQMFq/xCTaSH+swMS \
--weights 1 1 1 \
--threshold 1 \
--sigs \
AIYbCXAhPmILpWq6xsEY/Nu310Kednlb60Qcd/nD+u2WCXE/FvSXNRUQW9OQKGqt2CeskPyv2SEhaKMZ8gLkdQ8mmO01tDJz7vn6/2dqh+WEcmx7I/NKn8H6ornbk+HM4g==
 */

/*
weights + threshold = 1
const pubKeys: string[] = [
  "ACaY7TW0MnPu+fr/Z2qH5YRybHsj80qfwfqiuduT4czi",
  "ABr818VXt+6PLPRoA7QnsHBfRpKJdWZPjt7ppiTl6Fkq",
  "ALDE3sq5JZOj3Hmo/UeUv14zi4TFQMFq/xCTaSH+swMS",
];

const sigs: SerializedSignature[] = [
  "AIYbCXAhPmILpWq6xsEY/Nu310Kednlb60Qcd/nD+u2WCXE/FvSXNRUQW9OQKGqt2CeskPyv2SEhaKMZ8gLkdQ8mmO01tDJz7vn6/2dqh+WEcmx7I/NKn8H6ornbk+HM4g=="
];
*/
