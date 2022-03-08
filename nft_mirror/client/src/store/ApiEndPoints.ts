/// Endpoints for the NFT minting and signing
/// use demo data for testing
import { config  } from './index';
import { mintSuiNFTDemo, signNFTDemo, fetchDataNFTDemo } from '../store/demoAPi'
import { mapDatafromApi } from '../util';

/**
 * 
 * @param wallectAddress 
 * @returns 
 */
const fetchNFTData = async (wallectAddress:string) => {
    try {
        if(!wallectAddress) throw new Error("Wallet address is required")
        const urlAddress = `${config.url}nfts/ethereum/${wallectAddress}`;
        const response:any = await fetch(urlAddress);
        return mapDatafromApi(response.json().results);
    } catch (err) {
        throw err
    } 
}

export const fetchNFTDataByAddress = async (wallectAddress:string) => {
    try {
        const response = await config.demo ? fetchDataNFTDemo(wallectAddress) : fetchNFTData(wallectAddress)
        return response
    } catch (err) {
        throw err
    }
} 

const mintSuiNFT = async (reqObj) => {
    try {
        const apiEndPoint = config.url;
        const urlAddress = `${apiEndPoint}airdrop`;
        const response:any = await fetch(urlAddress, {
            method: 'POST',
            mode: 'cors',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify(reqObj)
        })

        return response.json()
    } catch (err) {
        throw err
    }
}
const signNFT = async (signObj) => {
    try {
        let nftJson = JSON.stringify(signObj)
        const signature = await window.ethereum.request({ method: 'personal_sign', params: [ nftJson ] })
        return signature
    } catch (err) {
        throw err
    }
}

export const startSigning = async (signObj) => {
    try {

        const response:any = await config.demo ? signNFTDemo(signObj) : signNFT(signObj);
        return response
    } catch (err) {
        throw err
    }
}

export const startMinting = async (reqObj) => {
    try {
        const response:any = await config.demo ? mintSuiNFTDemo(reqObj) : mintSuiNFT(reqObj);
        return response
    } catch (err) {
        throw err
    }
}