import { Ed25519Keypair } from '@mysten/sui.js';
import { randomBytes } from '@noble/hashes/utils';
import { toBufferBE, toBigIntBE } from 'bigint-buffer';
import { base64url, decodeJwt } from 'jose';
import { poseidon4 } from 'poseidon-lite';
import Browser from 'webextension-polyfill';

const clientID =
    '946731352276-pk5glcg8cqo38ndb39h7j093fpsphusu.apps.googleusercontent.com';
const redirectUri = 'https://ainifalglpinojmobpmeblikiopckbbm.chromiumapp.org/'; // TODO: use Browser.identity.getRedirectURL() for prod
const nonceLen = Math.ceil(256 / 6);

function generateNonce(
    ephemeralPublicKey: bigint,
    maxEpoch: number,
    randomness: bigint
) {
    const eph_public_key_0 = ephemeralPublicKey / 2n ** 128n;
    const eph_public_key_1 = ephemeralPublicKey % 2n ** 128n;
    const bignum = poseidon4([
        eph_public_key_0,
        eph_public_key_1,
        maxEpoch,
        randomness,
    ]);
    const Z = toBufferBE(bignum, 32); // padded to 32 bytes
    const nonce = base64url.encode(Z);
    if (nonce.length !== nonceLen) {
        throw new Error(
            `Length of nonce ${nonce} (${nonce.length}) is not equal to ${nonceLen}`
        );
    }
    return nonce;
}

export async function createZkAccount(currentEpoch: number) {
    const ephemeralKeypair = new Ed25519Keypair();
    const randomness = toBigIntBE(Buffer.from(randomBytes(16)));
    const nonce = generateNonce(
        toBigIntBE(Buffer.from(ephemeralKeypair.getPublicKey().toBytes())),
        currentEpoch + 2,
        randomness
    );
    console.log(
        currentEpoch,
        ephemeralKeypair.getPublicKey().toSuiAddress(),
        nonce
    );
    await zkLogin(nonce);
}

export async function zkLogin(nonce: string, loginAccount?: string) {
    const params = new URLSearchParams();
    params.append('client_id', clientID);
    params.append('response_type', 'id_token');
    params.append('redirect_uri', redirectUri);
    params.append('scope', 'openid email');
    params.append('nonce', nonce);
    // This can be used for logins after the user has already connected a google account
    // and we need to make sure that the user logged in with the correct account
    if (loginAccount) {
        params.append('login_hint', 'test@mystenlabs.com');
    }
    const url = `https://accounts.google.com/o/oauth2/v2/auth?${params.toString()}`;
    const responseURL = new URL(
        await Browser.identity.launchWebAuthFlow({
            url,
            interactive: true,
        })
    );
    const responseParams = new URLSearchParams(
        responseURL.hash.replace('#', '')
    );
    const decodedJWT = decodeJwt(responseParams.get('id_token') || '');
    console.log(url, decodedJWT, responseParams);
}
