import { DAppInterface } from './DAppInterface';

Object.defineProperty(window, 'suiWallet', {
    enumerable: false,
    configurable: false,
    value: new DAppInterface(window),
});
