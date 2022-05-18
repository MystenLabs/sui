import { createContext, type Dispatch, type SetStateAction } from 'react';

import { Network } from './utils/api/DefaultRpcClient';

export const NetworkContext = createContext<
    [Network | string, Dispatch<SetStateAction<Network | string>>]
>([Network.Devnet, () => null]);
