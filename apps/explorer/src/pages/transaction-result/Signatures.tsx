// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiTransactionBlockResponse } from '@mysten/sui.js/client';
import {
	parseSerializedSignature,
	type SignatureScheme,
	type PublicKey,
} from '@mysten/sui.js/cryptography';
import { parsePartialSignatures } from '@mysten/sui.js/multisig';
import { toB64, normalizeSuiAddress } from '@mysten/sui.js/utils';
import { publicKeyFromRawBytes } from '@mysten/sui.js/verify';
import { Text } from '@mysten/ui';

import { DescriptionItem, DescriptionList } from '~/ui/DescriptionList';
import { AddressLink } from '~/ui/InternalLink';
import { TabHeader } from '~/ui/Tabs';

type SignaturePubkeyPair = {
	signatureScheme: SignatureScheme;
	signature: Uint8Array;
} & ({ address: string } | { publicKey: PublicKey });

function SignaturePanel({
	title,
	signature: data,
}: {
	title: string;
	signature: SignaturePubkeyPair;
}) {
	const { signature, signatureScheme } = data;
	return (
		<TabHeader title={title}>
			<DescriptionList>
				<DescriptionItem title="Scheme" align="start" labelWidth="sm">
					<Text variant="pBody/medium" color="steel-darker">
						{signatureScheme}
					</Text>
				</DescriptionItem>
				<DescriptionItem title="Address" align="start" labelWidth="sm">
					<AddressLink
						noTruncate
						address={'address' in data ? data.address : data.publicKey.toSuiAddress()}
					/>
				</DescriptionItem>
				{'publicKey' in data ? (
					<DescriptionItem title="Sui Public Key" align="start" labelWidth="sm">
						<Text variant="pBody/medium" color="steel-darker">
							{data.publicKey.toSuiPublicKey()}
						</Text>
					</DescriptionItem>
				) : null}
				<DescriptionItem title="Signature" align="start" labelWidth="sm">
					<Text variant="pBody/medium" color="steel-darker">
						{toB64(signature)}
					</Text>
				</DescriptionItem>
			</DescriptionList>
		</TabHeader>
	);
}

function getSignatureFromAddress(signatures: SignaturePubkeyPair[], suiAddress: string) {
	return signatures.find(
		(signature) =>
			('address' in signature ? signature.address : signature.publicKey.toSuiAddress()) ===
			normalizeSuiAddress(suiAddress),
	);
}

function getSignaturesExcludingAddress(
	signatures: SignaturePubkeyPair[],
	suiAddress: string,
): SignaturePubkeyPair[] {
	return signatures.filter(
		(signature) =>
			('address' in signature ? signature.address : signature.publicKey.toSuiAddress()) !==
			normalizeSuiAddress(suiAddress),
	);
}
interface Props {
	transaction: SuiTransactionBlockResponse;
}

export function Signatures({ transaction }: Props) {
	const sender = transaction.transaction?.data.sender;
	const gasData = transaction.transaction?.data.gasData;
	const transactionSignatures = transaction.transaction?.txSignatures;

	if (!transactionSignatures) return null;

	const isSponsoredTransaction = gasData?.owner !== sender;

	const deserializedTransactionSignatures = transactionSignatures
		.map((signature) => {
			const parsed = parseSerializedSignature(signature);
			if (parsed.signatureScheme === 'MultiSig') {
				return parsePartialSignatures(parsed.multisig);
			}
			if (parsed.signatureScheme === 'ZkLogin') {
				return {
					signatureScheme: parsed.signatureScheme,
					address: parsed.zkLogin.address,
					signature: parsed.signature,
				};
			}

			return {
				...parsed,
				publicKey: publicKeyFromRawBytes(parsed.signatureScheme, parsed.publicKey),
			};
		})
		.flat();

	const userSignatures = isSponsoredTransaction
		? getSignaturesExcludingAddress(deserializedTransactionSignatures, gasData!.owner)
		: deserializedTransactionSignatures;

	const sponsorSignature = isSponsoredTransaction
		? getSignatureFromAddress(deserializedTransactionSignatures, gasData!.owner)
		: null;

	return (
		<div className="flex flex-col gap-8">
			{userSignatures.length > 0 && (
				<div className="flex flex-col gap-8">
					{userSignatures.map((signature, index) => (
						<div key={index}>
							<SignaturePanel title="User Signature" signature={signature} />
						</div>
					))}
				</div>
			)}

			{sponsorSignature && (
				<SignaturePanel title="Sponsor Signature" signature={sponsorSignature} />
			)}
		</div>
	);
}
