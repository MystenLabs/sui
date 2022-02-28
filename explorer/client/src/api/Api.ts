import { stringifyUrl } from 'query-string';

import type { SuiApi } from './api-definition';
import type { StringifiableRecord } from 'query-string';

type ApiPaths = keyof SuiApi;
type GetPathMethods<Path extends string> = Path extends ApiPaths
    ? keyof SuiApi[Path]
    : never;
type GetPathData<
    Path extends string,
    Method extends GetPathMethods<Path>,
    DataKey
> = Path extends ApiPaths
    ? DataKey extends keyof SuiApi[Path][Method]
        ? SuiApi[Path][Method][DataKey]
        : never
    : never;

const API_PATH = '/api/';

export async function request<
    Path extends ApiPaths,
    Method extends GetPathMethods<Path>
>({
    path,
    method,
    queryParams,
    requestParams,
}: {
    path: Path;
    method: Method;
    queryParams?: GetPathData<Path, Method, 'queryParams'>;
    requestParams?: GetPathData<Path, Method, 'requestParams'>;
}): Promise<GetPathData<Path, Method, 200>> {
    const resp = await fetch(
        stringifyUrl({
            url: `${API_PATH}${path}`,
            query: queryParams as unknown as StringifiableRecord,
        }),
        {
            headers: {
                'Content-Type': 'application/json',
                Accepts: 'application/json',
            },
            method,
            body: requestParams ? JSON.stringify(requestParams) : undefined,
        }
    );
    if (resp.ok) {
        return await resp.json();
    }
    // TODO: better errors include status, extra info as json
    throw new Error(await resp.text());
}
