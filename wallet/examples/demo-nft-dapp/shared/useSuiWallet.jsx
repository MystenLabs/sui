import { useEffect, useState } from 'react';

export const useSuiWallet = () => {
    const [wallet, setWallet] = useState(null);
    const [loaded, setLoaded] = useState(false);

    useEffect(() => {
        const cb = () => {
            setLoaded(true);
            setWallet(window.suiWallet);
        };
        if (window.suiWallet) {
            cb();
            return;
        }
        window.addEventListener('load', cb);
        return () => {
            window.removeEventListener('load', cb);
        };
    }, []);
    return wallet || (loaded ? false : null);
};
