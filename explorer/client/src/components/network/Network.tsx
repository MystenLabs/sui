import { useCallback } from 'react';

import { useNetwork } from '../../app/App';

export default function Network() {
    const [network, setNetwork] = useNetwork();
    const handleClick = useCallback(
        () => setNetwork(network === 'Devnet' ? 'Testnet' : 'Devnet'),
        [setNetwork, network]
    );

    return <div onClick={handleClick}>Click Here</div>;
}
