import { useCallback, useContext } from 'react';

import { NetworkContext } from '../../context';
import { Network } from '../../utils/api/DefaultRpcClient';

export default function NetworkSelect() {
    const [network, setNetwork] = useContext(NetworkContext);
    const handleClick = useCallback(
        () =>
            setNetwork(
                network === Network.Devnet ? Network.Local : Network.Devnet
            ),
        [setNetwork, network]
    );

    return <div onClick={handleClick}>Click Here</div>;
}
