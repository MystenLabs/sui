import { createContext, type Dispatch, type SetStateAction } from 'react';

export const NetworkContext = createContext<
    [string, Dispatch<SetStateAction<string>>]
>(['', () => null]);
