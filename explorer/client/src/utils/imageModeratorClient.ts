//import { setInterval } from 'timers/promises';
import { Network } from './api/rpcSetting';
import { IS_LOCAL_ENV, IS_STATIC_ENV } from './envUtil';

const ENV_STUBS_IMG_CHECK = IS_STATIC_ENV || IS_LOCAL_ENV;

const ENDPOINTS = {
    [Network.Local]: 'http://127.0.0.1:9200',
    [Network.Devnet]: 'http://127.0.0.1:9200',
    // TODO - stand up this real instance
    //[Network.Devnet]: 'https://explorer-imgmod.sui.io:443',
};

function getHost(network: Network | string): string {
    if (Object.keys(ENDPOINTS).includes(network))
        return ENDPOINTS[network as Network];
    return '';
}

export type ImageCheckResponse = { ok: boolean };

export interface IImageModClient {
    checkImage(url: string): Promise<ImageCheckResponse>;
}

export class ImageModClient implements IImageModClient {
    private readonly host: string;
    private readonly imgEndpoint: string;

    constructor(network: Network | string) {
        this.host = getHost(network);
        this.imgEndpoint = `${this.host}/img`;
    }

    async checkImage(url: string): Promise<ImageCheckResponse> {
        if (ENV_STUBS_IMG_CHECK) return { ok: true };

        return (
            await fetch(this.imgEndpoint, {
                method: 'POST',
                headers: { 'content-type': 'application/json' },
                body: JSON.stringify({ url }),
            })
        ).json();
    }
}
