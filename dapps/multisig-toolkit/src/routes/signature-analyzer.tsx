// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { parseSerializedSignature, PublicKey, SignatureScheme } from '@mysten/sui/cryptography';
import { MultiSigPublicKey, parsePartialSignatures } from '@mysten/sui/multisig';
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

interface MultiSigInfo {
	publicKey: MultiSigPublicKey;
	threshold: number;
	participants: {
		publicKey: PublicKey;
		weight: number;
		suiAddress: string;
		keyType: string;
	}[];
}

// Helper function to determine key type from flag
function getKeyTypeFromFlag(flag: number): string {
	switch (flag) {
		case 0:
			return 'Ed25519';
		case 1:
			return 'Secp256k1';
		case 2:
			return 'Secp256r1';
		case 3:
			return 'MultiSig';
		case 5:
			return 'ZkLogin';
		default:
			return `Unknown (${flag})`;
	}
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

function MultiSigDetails({ multisigInfo }: { multisigInfo: MultiSigInfo }) {
	const multisigAddress = multisigInfo.publicKey.toSuiAddress();
	const multisigPubkey = multisigInfo.publicKey.toSuiPublicKey();

	return (
		<Card className="border-primary">
			<CardHeader>
				<CardTitle>MultiSig Configuration</CardTitle>
				<CardDescription>Combined MultiSig Public Key Information</CardDescription>
			</CardHeader>
			<CardContent>
				<div className="flex flex-col gap-4">
					<div className="flex flex-col gap-1.5">
						<div className="font-bold">MultiSig Address</div>
						<div className="bg-muted rounded text-sm font-mono p-2 break-all">
							{multisigAddress}
						</div>
					</div>

					<div className="flex flex-col gap-1.5">
						<div className="font-bold">MultiSig Public Key</div>
						<div className="bg-muted rounded text-sm font-mono p-2 break-all">{multisigPubkey}</div>
					</div>

					<div className="flex flex-col gap-1.5">
						<div className="font-bold">Threshold</div>
						<div className="bg-muted rounded text-sm font-mono p-2">{multisigInfo.threshold}</div>
					</div>

					<div className="flex flex-col gap-1.5">
						<div className="font-bold">Participants ({multisigInfo.participants.length})</div>
						<div className="space-y-2">
							{multisigInfo.participants.map((participant, index) => (
								<div key={index} className="bg-muted rounded p-3">
									<div className="flex justify-between items-start mb-2">
										<span className="font-semibold">Participant #{index + 1}</span>
										<div className="flex gap-2">
											<span className="bg-secondary/50 text-secondary-foreground px-2 py-1 rounded text-sm">
												{participant.keyType}
											</span>
											<span className="bg-primary/10 text-primary px-2 py-1 rounded text-sm">
												Weight: {participant.weight}
											</span>
										</div>
									</div>
									<div className="space-y-1 text-sm">
										<div>
											<span className="text-muted-foreground">Address:</span>{' '}
											<span className="font-mono">{participant.suiAddress}</span>
										</div>
										<div>
											<span className="text-muted-foreground">Public Key:</span>{' '}
											<span className="font-mono break-all">
												{participant.publicKey.toBase64()}
											</span>
										</div>
									</div>
								</div>
							))}
						</div>
					</div>
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
	const [multisigInfo, setMultisigInfo] = useState<MultiSigInfo | null>(null);

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
					setMultisigInfo(null);

					try {
						const parsedSignature = parseSerializedSignature(signature);

						if (parsedSignature.signatureScheme === 'MultiSig') {
							// Create MultiSigPublicKey instance to access all the metadata
							const multiSigPubKey = new MultiSigPublicKey(parsedSignature.multisig.multisig_pk);

							// Get all participants with their weights
							const participants = multiSigPubKey.getPublicKeys().map(({ publicKey, weight }) => ({
								publicKey,
								weight,
								suiAddress: publicKey.toSuiAddress(),
								keyType: (publicKey as any).keyType || getKeyTypeFromFlag(publicKey.flag()),
							}));

							// Store multisig information
							setMultisigInfo({
								publicKey: multiSigPubKey,
								threshold: multiSigPubKey.getThreshold(),
								participants,
							});

							// Parse individual signatures
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
				{multisigInfo && <MultiSigDetails multisigInfo={multisigInfo} />}

				{listSignaturePubKeys && listSignaturePubKeys.length > 0 && (
					<div className="flex flex-col gap-4">
						<h3 className="text-2xl font-bold">Individual Signatures</h3>
						{listSignaturePubKeys.map((signature, index) => (
							<Signature key={index} index={index} signature={signature} />
						))}
					</div>
				)}
			</div>
		</div>
	);
}
