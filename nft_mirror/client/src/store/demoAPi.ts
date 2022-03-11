import { mapDatafromApi } from '../util';
/// demo api endpoints and response data
/// remove this file in production

function resolveAfter2Seconds() {
    return new Promise(resolve => {
        setTimeout(() => {
        resolve('resolved');
        }, 2000);
    });
}



export const fetchDataNFTDemo = async (wallectAddress:string) => {
    try {
        if(!wallectAddress) throw new Error("Wallet address is required")
        const demoNFTsData = {
            "results": [
                {
                    "token": {
                        "contract_address": "0x226bf5293692610692e2c996c9875c914d2a7f73",
                        "name": "RTFKT Space Pod",
                        "token_id": "0x05",
                        "media_uri": "ipfs://QmYLKT7uM1psPr8j7fH8dzSfm4ga5gmTnf72eqbPcism5u"
                    },
                    "claim_status": "none"
                },
                {
                    "token": {
                        "contract_address": "0x3fe1a4c1481c8351e91b64d5c398b159de07cbc5",
                        "name": "SupDuck 4207",
                        "token_id": "0x000000000000000000000000000000000000000000000000000000000000106f",
                        "media_uri": "https://ipfs.io/ipfs/Qmc4m3zdxpKNEkEAKanuZjfxVGLwnVGVABz9iJyriBGza3"
                    },
                    "claim_status": "none"
                },
                {
                    "token": {
                        "contract_address": "0x42f1654b8eeb80c96471451b1106b63d0b1a9fe1",
                        "name": "Chubbipig #2464",
                        "token_id": "0x00000000000000000000000000000000000000000000000000000000000009a0",
                        "media_uri": "https://chubbifren.s3.us-east-2.amazonaws.com/images/2464.png"
                    },
                    "claim_status": "none"
                },
                {
                    "token": {
                        "contract_address": "0x42f1654b8eeb80c96471451b1106b63d0b1a9fe1",
                        "name": "Chubbicat #7016",
                        "token_id": "0x0000000000000000000000000000000000000000000000000000000000001b68",
                        "media_uri": "https://chubbifren.s3.us-east-2.amazonaws.com/images/7016.png"
                    },
                    "claim_status": "none"
                },
                {
                    "token": {
                        "contract_address": "0x49cf6f5d44e70224e2e23fdcdd2c053f30ada28b",
                        "name": "CloneX #18172",
                        "token_id": "0x00000000000000000000000000000000000000000000000000000000000007b0",
                        "media_uri": "https://clonex-assets.rtfkt.com/images/1968.png"
                    },
                    "claim_status": "none"
                },
                {
                    "token": {
                        "contract_address": "0x49cf6f5d44e70224e2e23fdcdd2c053f30ada28b",
                        "name": "CloneX #15493",
                        "token_id": "0x0000000000000000000000000000000000000000000000000000000000000fd6",
                        "media_uri": "https://clonex-assets.rtfkt.com/images/4054.png"
                    },
                    "claim_status": "none"
                },
                {
                    "token": {
                        "contract_address": "0x49cf6f5d44e70224e2e23fdcdd2c053f30ada28b",
                        "name": "CloneX #4029",
                        "token_id": "0x00000000000000000000000000000000000000000000000000000000000039b6",
                        "media_uri": "https://clonex-assets.rtfkt.com/images/14774.png"
                    },
                    "claim_status": "none"
                },
                {
                    "token": {
                        "contract_address": "0x57f1887a8bf19b14fc0df6fd9b2acc9af147ea85",
                        "name": "mmxyz.eth",
                        "token_id": "0x6f42c1cec21e7d78597c924b7a2e263ede6059fc656716f0222efe39eeb63d85"
                    },
                    "claim_status": "none"
                },
                {
                    "token": {
                        "contract_address": "0x5ab81e38b14faa61a699af1bccd1fe5ecab20fae",
                        "name": "Alpha Finance 2.0 x APY.Vision NFT",
                        "token_id": "0x20",
                        "media_uri": "ipfs://ipfs/QmebhVHaTiMKvdpX6dFR7w8PffiLcWao31myrKiEbuFVXQ/image.png"
                    },
                    "claim_status": "none"
                },
                {
                    "token": {
                        "contract_address": "0x86825dfca7a6224cfbd2da48e85df2fc3aa7c4b1",
                        "name": "RTFKT - MNLTH 🗿",
                        "token_id": "0x01",
                        "media_uri": "ipfs://QmWYzoYLgsQpr294KywUiHA8EwfPn5AgaN4RFgqDmSGuaq"
                    },
                    "claim_status": "none"
                },
                {
                    "token": {
                        "contract_address": "0x933492b6b7038a7e4f14b64defe40463f9bc3508",
                        "name": "",
                        "token_id": "0x00000000000000000000000000000000000000000000000000000000000004bf"
                    },
                    "claim_status": "none"
                },
                {
                    "token": {
                        "contract_address": "0x97597002980134bea46250aa0510c9b90d87a587",
                        "name": "Runner #6005",
                        "token_id": "0x0000000000000000000000000000000000000000000000000000000000001775",
                        "media_uri": "https://img.chainrunners.xyz/api/v1/tokens/png/6005"
                    },
                    "claim_status": "none"
                },
                {
                    "token": {
                        "contract_address": "0xb7be4001bff2c5f4a61dd2435e4c9a19d8d12343",
                        "name": "RTFKT LOOT Pod",
                        "token_id": "0x01",
                        "media_uri": "ipfs://QmZMUiC6G2kRRyk7oCacVsuqcornwVM6LdPEWibyAc8xMu"
                    },
                    "claim_status": "none"
                },
                {
                    "token": {
                        "contract_address": "0xbc4ca0eda7647a8ab7c2061c2e118a18a936f13d",
                        "name": "",
                        "token_id": "0x00000000000000000000000000000000000000000000000000000000000022ae",
                        "media_uri": "ipfs://QmedGpmBhyB6RbPcKM8Y8B8EyPBEKMpaMj5BNPTFfjtKAM"
                    },
                    "claim_status": "none"
                },
                {
                    "token": {
                        "contract_address": "0xbc4ca0eda7647a8ab7c2061c2e118a18a936f13d",
                        "name": "",
                        "token_id": "0x00000000000000000000000000000000000000000000000000000000000022e9",
                        "media_uri": "ipfs://QmYjEDqhb9dtoiJShEaUaVceoTa7hf25dhH8JYxjtF3HJ6"
                    },
                    "claim_status": "none"
                },
                {
                    "token": {
                        "contract_address": "0xed5af388653567af2f388e6224dc7c4b3241c544",
                        "name": "Azuki #9289",
                        "token_id": "0x0000000000000000000000000000000000000000000000000000000000002449",
                        "media_uri": "https://ikzttp.mypinata.cloud/ipfs/QmYDvPAXtiJg7s8JdRBSLWdgSphQdac8j1YuQNNxcGE1hg/9289.png"
                    },
                    "claim_status": "none"
                }
            ]
        }
       
        await resolveAfter2Seconds()
       /// format data from api
        return mapDatafromApi(demoNFTsData.results);
    } catch (err) {
        throw err
    }
}


export const signNFTDemo = async (suiWalletAddress) => {
    try {

        await resolveAfter2Seconds();
        /// ramdomly throw error! to test error handling 
        /*if([true, false][Math.round(Math.random())]){
            throw new Error("Signing failed")
        }*/
        return"0x21fbf0696d5e0aa2ef41a2b4ffb623bcaf070461d61cf7251c74161f82fec3a4370854bc0a34b3ab487c1bc021cd318c734c51ae29374f2beb0e6f2dd49b4bf41c"
    } catch (error) {
        throw error
    }
    
}


  

export const mintSuiNFTDemo = async (signature) => {
    await resolveAfter2Seconds();
    const success = {
            "source_chain": "ethereum",
            "source_contract_address": "0xBC4CA0EdA7647A8aB7C2061c2E118A18a936f13D",
            "source_token_id": "101",
            "sui_explorer_link": "https://djgd7fpxio1yh.cloudfront.net/objects/7bc832ec31709638cd8d9323e90edf332gff4389"
        }
    try {
        /// ramdomly throw error! to test error handling
       /*if([true, false][Math.round(Math.random())]){
            throw new Error("Validation failed")
        } */ 
        return success
    } catch (error) {
        throw ( {
            "message": "Validation failed",
            "details": {
                "requestBody": {
                "message": "id is an excess property and therefore not allowed",
                "value": "52907745-7672-470e-a803-a2f8feb52944"
                }
            }
            } );
    }
}