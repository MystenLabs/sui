import { createContext, type Dispatch, type SetStateAction } from 'react';

import { Network } from './utils/api/DefaultRpcClient';

export const NetworkContext = createContext<
    [Network, Dispatch<SetStateAction<Network>>]
>([Network.Devnet, () => null]);
