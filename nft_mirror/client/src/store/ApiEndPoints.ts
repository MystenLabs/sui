/// Endpoints for the NFT minting and signing
/// use demo data for testing
import { config } from "./index";
// import { mintSuiNFTDemo, signNFTDemo, fetchDataNFTDemo } from '../store/demoAPi'
import { mapDatafromApi } from "../util";

/**
 *
 * @param wallectAddress
 * @returns
 */
const fetchNFTData = async (wallectAddress: string) => {
  try {
    if (!wallectAddress) throw new Error("Wallet address is required");
    const urlAddress = `${config.url}nfts/ethereum/${wallectAddress}`;
    const response: any = await fetch(urlAddress);
    const data = await response.json();
    return mapDatafromApi(data.results);
  } catch (err) {
    throw err;
  }
};

export const fetchNFTDataByAddress = async (wallectAddress: string) => {
  try {
    //? fetchDataNFTDemo(wallectAddress) :
    const response = await fetchNFTData(wallectAddress);
    return response;
  } catch (err) {
    throw err;
  }
};

const mintSuiNFT = async (reqObj) => {
  try {
    const apiEndPoint = config.url;
    const urlAddress = `${apiEndPoint}airdrop`;
    const response: any = await fetch(urlAddress, {
      method: "POST",
      mode: "cors",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(reqObj),
    });
    const data = await response.json();
    return data.json();
  } catch (err) {
    throw err;
  }
};
const signNFT = async (from: string, msgParams: any) => {
  try {
    const signature = await window.ethereum.request({
      method: "eth_signTypedData_v4",
      params: [from, msgParams],
    });
    return signature;
  } catch (err) {
    console.log(err.message);
    throw err;
  }
};

/// Endpoints for the NFT minting and signing
///  intermedate function to call signNFTDemo and signNFT base on config.demo
export const startSigning = async (from, msgParams) => {
  try {
    //To show config.demo ? signNFTDemo(msgParams) :
    const response: any = await signNFT(from, msgParams);
    return response;
  } catch (err) {
    throw err;
  }
};

export const startMinting = async (reqObj) => {
  try {
    /// Minting- To use demo data please use mintSuiNFTDemo(reqObj)
    const response: any = await mintSuiNFT(reqObj);
    /// await config.demo ? mintSuiNFTDemo(reqObj) : mintSuiNFT(reqObj);
    return response;
  } catch (err) {
    throw err;
  }
};
