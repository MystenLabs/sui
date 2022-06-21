import { IS_LOCAL_ENV, IS_STATIC_ENV } from './envUtil';

const ENV_STUBS_IMG_CHECK = IS_STATIC_ENV || IS_LOCAL_ENV;
const HOST = 'https://imgmod.sui.io';

export type ImageCheckResponse = { ok: boolean };

export interface IImageModClient {
    checkImage(url: string): Promise<ImageCheckResponse>;
}

export class ImageModClient implements IImageModClient {
    private readonly imgEndpoint: string;

    constructor() {
        this.imgEndpoint = `${HOST}/img`;
    }

    async checkImage(url: string): Promise<ImageCheckResponse> {
        // static and local environments always allow images without checking
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
