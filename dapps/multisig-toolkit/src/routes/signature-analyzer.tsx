// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { parseSerializedSignature, PublicKey, SignatureScheme } from '@mysten/sui/cryptography';
import { parsePartialSignatures } from '@mysten/sui/multisig';
import { toBase64 } from '@mysten/sui/utils';
import { publicKeyFromRawBytes } from '@mysten/sui/verify';
import { AlertCircle } from 'lucide-react';
import { useState } from 'react';

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';

interface SignaturePubkeyPair {
	signatureScheme: SignatureScheme;
	publicKey: PublicKey;
	signature: Uint8Array;
}

/*
MultiSig (v1)
AwEAhhsJcCE+YgularrGwRj827fXQp52eVvrRBx3+cP67ZYJcT8W9Jc1FRBb05Aoaq3YJ6yQ/K/ZISFooxnyAuR1DxI6MAAAAQAAAAAAAAAQAAAAAAADLEFDYVk3VFcwTW5QdStmci9aMnFINVlSeWJIc2o4MHFmd2ZxaXVkdVQ0Y3ppASxBQnI4MThWWHQrNlBMUFJvQTdRbnNIQmZScEtKZFdaUGp0N3BwaVRsNkZrcQEsQUxERTNzcTVKWk9qM0htby9VZVV2MTR6aTRURlFNRnEveENUYVNIK3N3TVMBAQA=
*/

/*
MultiSig (v2)
AwIAvlJnUP0iJFZL+QTxkKC9FHZGwCa5I4TITHS/QDQ12q1sYW6SMt2Yp3PSNzsAay0Fp2MPVohqyyA02UtdQ2RNAQGH0eLk4ifl9h1I8Uc+4QlRYfJC21dUbP8aFaaRqiM/f32TKKg/4PSsGf9lFTGwKsHJYIMkDoqKwI8Xqr+3apQzAwADAFriILSy9l6XfBLt5hV5/1FwtsIsAGFow3tefGGvAYCDAQECHRUjB8a3Kw7QQYsOcM2A5/UpW42G9XItP1IT+9I5TzYCADtqJ7zOtqQtYqOo0CpvDXNlMhV3HeJDpjrASKGLWdopAwMA
*/

/*
Single Sig
AIYbCXAhPmILpWq6xsEY/Nu310Kednlb60Qcd/nD+u2WCXE/FvSXNRUQW9OQKGqt2CeskPyv2SEhaKMZ8gLkdQ8mmO01tDJz7vn6/2dqh+WEcmx7I/NKn8H6ornbk+HM4g==
*/

function Signature({ signature, index }: { signature: SignaturePubkeyPair; index: number }) {
	const suiAddress = signature.publicKey.toSuiAddress();

	const pubkey_base64_sui_format = signature.publicKey.toSuiPublicKey();

	const pubkey = signature.publicKey.toBase64();
	const scheme = signature.signatureScheme.toString();

	const details = [
		{ label: 'Signature Public Key', value: pubkey },
		{ label: 'Sui Format Public Key ( flag | pk )', value: pubkey_base64_sui_format },
		{ label: 'Sui Address', value: suiAddress },
		{ label: 'Signature', value: toBase64(signature.signature) },
	];

	return (
		<Card>
			<CardHeader>
				<CardTitle>Signature #{index}</CardTitle>
				<CardDescription>{scheme}</CardDescription>
			</CardHeader>
			<CardContent>
				<div className="flex flex-col gap-2">
					{details.map(({ label, value }, index) => (
						<div key={index} className="flex flex-col gap-1.5">
							<div className="font-bold">{label}</div>
							<div className="bg-muted rounded text-sm font-mono p-2 break-all">{value}</div>
						</div>
					))}
				</div>
			</CardContent>
		</Card>
	);
}

export default function SignatureAnalyzer() {
	const [signature, setSignature] = useState('');
	const [error, setError] = useState<Error | null>(null);
	const [listSignaturePubKeys, setListSignaturePubkeys] = useState<SignaturePubkeyPair[] | null>(
		null,
	);

	return (
		<div className="flex flex-col gap-4">
			<h2 className="scroll-m-20 text-4xl font-extrabold tracking-tight lg:text-5xl">
				Signature Analyzer
			</h2>

			{error && (
				<Alert variant="destructive">
					<AlertCircle className="h-4 w-4" />
					<AlertTitle>Error</AlertTitle>
					<AlertDescription>{error.message}</AlertDescription>
				</Alert>
			)}

			<form
				className="flex flex-col gap-4"
				onSubmit={async (e) => {
					e.preventDefault();
					setError(null);

					try {
						const parsedSignature = parseSerializedSignature(signature);

						if (parsedSignature.signatureScheme === 'MultiSig') {
							const partialSignatures = parsePartialSignatures(parsedSignature.multisig);

							setListSignaturePubkeys(
								partialSignatures.map((signature) => {
									return {
										signatureScheme: signature.signatureScheme,
										publicKey: signature.publicKey,
										signature: signature.signature,
									};
								}),
							);
						} else {
							setListSignaturePubkeys([
								{
									signatureScheme: parsedSignature.signatureScheme,
									publicKey: publicKeyFromRawBytes(
										parsedSignature.signatureScheme,
										parsedSignature.publicKey,
									),
									signature: parsedSignature.signature,
								},
							]);
						}
					} catch (e) {
						setError(e as Error);
					}
				}}
			>
				<div className="grid w-full gap-1.5">
					<Label htmlFor="bytes">Signature Bytes (base64 encoded)</Label>
					<Textarea
						id="bytes"
						rows={4}
						value={signature}
						onChange={(e) => setSignature(e.target.value)}
					/>
				</div>
				<div>
					<Button type="submit">Analyze Signature</Button>
				</div>
			</form>

			<div className="flex flex-col gap-6 mt-6">
				{listSignaturePubKeys?.map((signature, index) => (
					<Signature index={index} signature={signature} />
				))}
			</div>
		</div>
	);
}
