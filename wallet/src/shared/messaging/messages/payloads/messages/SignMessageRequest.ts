import type { SignaturePubkeyPair } from '@mysten/sui.js';

export type SignMessageRequest = {
    id: string;
    approved: boolean | null;
    origin: string;
    originFavIcon?: string;
    message: Uint8Array;
    createdDate: string;
    signature?: SignaturePubkeyPair;
};
