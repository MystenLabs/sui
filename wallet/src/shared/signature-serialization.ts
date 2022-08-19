import {
    Base64DataBuffer,
    PublicKey,
    type SignaturePubkeyPair,
    type SignatureScheme,
} from '@mysten/sui.js';

export interface SerializedSignaturePubkeyPair {
    signatureScheme: string;
    signature: string;
    pubKey: string;
}

export function serializeSignaturePubkeyPair(
    signature: SignaturePubkeyPair
): SerializedSignaturePubkeyPair {
    return {
        signatureScheme: signature.signatureScheme,
        signature: signature.signature.toString(),
        pubKey: signature.pubKey.toBase64(),
    };
}

export function deserializeSignaturePubkeyPair(
    signature: SerializedSignaturePubkeyPair
): SignaturePubkeyPair {
    return {
        signatureScheme: signature.signatureScheme as SignatureScheme,
        signature: new Base64DataBuffer(signature.signature),
        pubKey: new PublicKey(signature.pubKey),
    };
}
