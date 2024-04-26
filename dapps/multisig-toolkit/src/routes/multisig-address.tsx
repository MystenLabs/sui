// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { PublicKey } from '@mysten/sui/cryptography';
import { MultiSigPublicKey } from '@mysten/sui/multisig';
import { publicKeyFromSuiBytes } from '@mysten/sui/verify';
import { useState } from 'react';
import { FieldValues, useFieldArray, useForm } from 'react-hook-form';
import { toast } from 'react-hot-toast';

import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';

/*
Pubkeys for playing with
ABr818VXt+6PLPRoA7QnsHBfRpKJdWZPjt7ppiTl6Fkq
ANRdB4M6Hj73R+gRM4N6zUPNidLuatB9uccOzHBc/0bP
*/

export default function MultiSigAddressGenerator() {
	const [msAddress, setMSAddress] = useState('');
	const { register, control, handleSubmit } = useForm({
		defaultValues: {
			pubKeys: [{ pubKey: '', weight: '' }],
			threshold: 1,
		},
	});
	const { fields, append, remove } = useFieldArray({
		control,
		name: 'pubKeys',
	});

	// Perform generation of multisig address
	const onSubmit = (data: FieldValues) => {
		try {
			const pks: { publicKey: PublicKey; weight: number }[] = [];
			data.pubKeys.forEach((item: any) => {
				const pk = publicKeyFromSuiBytes(item.pubKey);
				pks.push({ publicKey: pk, weight: item.weight });
			});
			const multiSigPublicKey = MultiSigPublicKey.fromPublicKeys({
				threshold: data.threshold,
				publicKeys: pks,
			});
			const multisigSuiAddress = multiSigPublicKey.toSuiAddress();
			setMSAddress(multisigSuiAddress);
		} catch (e: any) {
			toast.error(e?.message || 'Error generating MultiSig Address');
		}
	};

	// if you want to control your fields with watch
	// const watchResult = watch("pubKeys");
	// console.log(watchResult);

	// The following is useWatch example
	// console.log(useWatch({ name: "pubKeys", control }));

	return (
		<div className="flex flex-col gap-4">
			<h2 className="scroll-m-20 text-4xl font-extrabold tracking-tight lg:text-5xl">
				MultiSig Address Creator
			</h2>

			<form className="flex flex-col gap-4" onSubmit={handleSubmit(onSubmit)}>
				<p>The following demo allow you to create Sui MultiSig addresses.</p>
				<code className="overflow-x-auto">
					Sui Pubkeys for playing with
					<p>ABr818VXt+6PLPRoA7QnsHBfRpKJdWZPjt7ppiTl6Fkq</p>
					<p>ANRdB4M6Hj73R+gRM4N6zUPNidLuatB9uccOzHBc/0bP</p>
				</code>
				<ul className="grid w-full gap-1.5">
					{fields.map((item, index) => {
						return (
							<li key={item.id} className="grid grid-cols-3 gap-3">
								<input
									className="min-h-[80px] rounded-md border border-input bg-transparent px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
									{...register(`pubKeys.${index}.pubKey`, { required: true })}
									placeholder="Sui Public Key"
								/>

								<input
									className="min-h-[80px] rounded-md border border-input bg-transparent px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
									type="number"
									{...register(`pubKeys.${index}.weight`, { required: true })}
									placeholder="Weight"
								/>

								<div>
									<Button
										className="min-h-[80px] rounded-md border border-input px-3 py-2 text-sm padding-2"
										type="button"
										onClick={() => remove(index)}
									>
										Delete
									</Button>
								</div>
							</li>
						);
					})}
				</ul>
				<section>
					<Button
						type="button"
						onClick={() => {
							append({ pubKey: '', weight: '' });
						}}
					>
						New PubKey
					</Button>
				</section>
				<section>
					<label className="form-label min-h-[80px] rounded-md text-sm px-3 py-2 ring-offset-background">
						MultiSig Threshold Value:
					</label>
					<input
						className="min-h-[80px] rounded-md border border-input bg-transparent px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
						type="number"
						{...register(`threshold`, { valueAsNumber: true, required: true })}
					/>
				</section>

				{/* <input
					{...register('threshold', { valueAsNumber: true })}
					id="threshold"
					type="number"
					className="form-control"
				/> */}

				<Button type="submit">Create MultiSig Address</Button>
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
		</div>
	);
}

/*
➜  multisig-toolkit git:(jnaulty/multisig-create-address) ✗ sui keytool multi-sig-address --pks ABr818VXt+6PLPRoA7QnsHBfRpKJdWZPjt7ppiTl6Fkq ANRdB4M6Hj73R+gRM4N6zUPNidLuatB9uccOzHBc/0bP --weights 1 2 --threshold 2
MultiSig address: 0x27b17213bc702893bb3e92ba84071589a6331f35f066ad15b666b9527a288c16
Participating parties:
                Sui Address                 |                Public Key (Base64)                 | Weight
----------------------------------------------------------------------------------------------------
 0x504f656b7bc467f6eb1d05dc26447477921f05e5ea88c5715682ad28835268ce | ABr818VXt+6PLPRoA7QnsHBfRpKJdWZPjt7ppiTl6Fkq  |   1
 0x611f6a023c5d1c98b4de96e9da64daffaeb372fed0176536168908e50f6e07c0 | ANRdB4M6Hj73R+gRM4N6zUPNidLuatB9uccOzHBc/0bP  |   2
➜  multisig-toolkit git:(jnaulty/multisig-create-address) ✗ sui keytool multi-sig-address --pks ABr818VXt+6PLPRoA7QnsHBfRpKJdWZPjt7ppiTl6Fkq ANRdB4M6Hj73R+gRM4N6zUPNidLuatB9uccOzHBc/0bP --weights 1 1 --threshold 2
MultiSig address: 0x9134bd58a25a6b48811d1c65770dd1d01e113931ed35c13f1a3c26ed7eccf9bc
Participating parties:
                Sui Address                 |                Public Key (Base64)                 | Weight
----------------------------------------------------------------------------------------------------
 0x504f656b7bc467f6eb1d05dc26447477921f05e5ea88c5715682ad28835268ce | ABr818VXt+6PLPRoA7QnsHBfRpKJdWZPjt7ppiTl6Fkq  |   1
 0x611f6a023c5d1c98b4de96e9da64daffaeb372fed0176536168908e50f6e07c0 | ANRdB4M6Hj73R+gRM4N6zUPNidLuatB9uccOzHBc/0bP  |   1
➜  multisig-toolkit git:(jnaulty/multisig-create-address) ✗ sui keytool multi-sig-address --pks ABr818VXt+6PLPRoA7QnsHBfRpKJdWZPjt7ppiTl6Fkq ANRdB4M6Hj73R+gRM4N6zUPNidLuatB9uccOzHBc/0bP --weights 1 1 --threshold 1
MultiSig address: 0xda3f8c1ba647d63b89a396a64eeac835d25a59323a1b8fd4697424f62374b0de
Participating parties:
                Sui Address                 |                Public Key (Base64)                 | Weight
----------------------------------------------------------------------------------------------------
 0x504f656b7bc467f6eb1d05dc26447477921f05e5ea88c5715682ad28835268ce | ABr818VXt+6PLPRoA7QnsHBfRpKJdWZPjt7ppiTl6Fkq  |   1
 0x611f6a023c5d1c98b4de96e9da64daffaeb372fed0176536168908e50f6e07c0 | ANRdB4M6Hj73R+gRM4N6zUPNidLuatB9uccOzHBc/0bP  |   1
 */
