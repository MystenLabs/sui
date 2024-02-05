import { Signer } from '@mysten/sui.js/cryptography';
import { Secp256k1PublicKey } from '@mysten/sui.js/keypairs/secp256k1';

export class AWSKMSSigner extends Signer {
	#pk: Secp256k1PublicKey;

	constructor(publicKey: string) {
		super();
		this.#pk = new Secp256k1PublicKey(publicKey);
	}

	getKeyScheme() {
		return 'Secp256k1' as const;
	}

	getPublicKey() {
		return this.#pk;
	}

	sign() {
		// TODO: Implement
	}

	signData(): never {
		throw new Error('KMS Signer does not support sync signing');
	}
}
