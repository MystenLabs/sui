import { useCallback, useContext } from 'react';

import { NetworkContext } from '../../context';

export default function Network() {
    const [network, setNetwork] = useContext(NetworkContext);
    const handleClick = useCallback(
        () => setNetwork(network === 'Devnet' ? 'Testnet' : 'Devnet'),
        [setNetwork, network]
    );

    return <div onClick={handleClick}>Click Here</div>;
}
