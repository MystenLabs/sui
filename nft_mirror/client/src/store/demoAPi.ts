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
                    "contract_address": "0x090f688f0c11a8671c47d833af3cf965c30d3c35",
                    "name": "Deadfrenz Lab Access Pass (DF)",
                    "token_id": "0x02",
                    "media_uri": "ipfs://QmPrEMTsZWst1YzALnKzC3MeA9Qcc8uS1n8zUbNRg41NQA"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0x090f688f0c11a8671c47d833af3cf965c30d3c35",
                    "name": "Deadfrenz Lab Access Pass",
                    "token_id": "0x05",
                    "media_uri": "ipfs://QmPrEMTsZWst1YzALnKzC3MeA9Qcc8uS1n8zUbNRg41NQA"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0x1657e2200216ebacb92475b69d6bc0fdad48b068",
                    "name": "#2519",
                    "token_id": "0x00000000000000000000000000000000000000000000000000000000000009d7",
                    "media_uri": "ipfs://QmQ86mg5LSCS4j9AjQ6BQ5Q2yqtoGGVofjcZHdhMzZnuMb/2519.png"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0x2acab3dea77832c09420663b0e1cb386031ba17b",
                    "name": "DeadFellaz #3022",
                    "token_id": "0x0000000000000000000000000000000000000000000000000000000000000bce",
                    "media_uri": "https://gateway.pinata.cloud/ipfs/QmVHjkV7JaJSgBExJ1drnQHozqcQWioTyWKwphxzoMKBnA"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0x2acab3dea77832c09420663b0e1cb386031ba17b",
                    "name": "DeadFellaz #9992",
                    "token_id": "0x0000000000000000000000000000000000000000000000000000000000002708",
                    "media_uri": "https://gateway.pinata.cloud/ipfs/QmXZ8nLPtQvQpJMY3tVDYdmg1xXW1Cs8FCsY93nCBCyzc6"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0x4274675c3d8b3767e099288efb486f6838a3e532",
                    "name": "",
                    "token_id": "0x000000000000000000000000000000000000000000000000000000000000003e"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0x495f947276749ce646f68ac8c248420045cb7b5e",
                    "name": "Picasso Punk #0660",
                    "token_id": "0xb953b2f5a30b9897ef58a495051b373039f033af000000000003bc0000000001",
                    "media_uri": "https://lh3.googleusercontent.com/uVfEWkPYeeONt2HUx2CMmV4IfY3AkJ4Qy4sVda6GONpfAB7JZbSETOmdt63Mv6-2ntX6hMgGH_NLkTMLwTsHJ-ZYtN-C-TCB-_WOlrI"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0x57f1887a8bf19b14fc0df6fd9b2acc9af147ea85",
                    "name": "cheapbills.eth",
                    "token_id": "0x31c825ef8a880f4aa861382921a318c967f13dcd3c9e16e14bb70e643a3f748a"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0x57f1887a8bf19b14fc0df6fd9b2acc9af147ea85",
                    "name": "casinow.eth",
                    "token_id": "0x52da1b72df839d786f4afa9175d9f4f6a4793fe58004edc9cfabff4597736875"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0x57f1887a8bf19b14fc0df6fd9b2acc9af147ea85",
                    "name": "holypunks.eth",
                    "token_id": "0x58190089d60ccdc4f357e40964ad64f9fefebc0750a9738f799218532f96d65a"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0x57f1887a8bf19b14fc0df6fd9b2acc9af147ea85",
                    "name": "zens.eth",
                    "token_id": "0x97895b56545b866473f6a124320f4c536c45bc1faae6b9bb8590658272031896"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0x57f1887a8bf19b14fc0df6fd9b2acc9af147ea85",
                    "name": "diorofficial.eth",
                    "token_id": "0xd5b62f0c95f48dee226c56eeb37bd210df71b30926910a65009ee3a8f332a651"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0x60e4d786628fea6478f785a6d7e704777c86a7c6",
                    "name": "",
                    "token_id": "0x000000000000000000000000000000000000000000000000000000000000625b",
                    "media_uri": "ipfs://QmThDrFXh2UU4wCdwZbZwY6Fz4RB65wF2gJCKeJQMq9CRf"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0x7afeda4c714e1c0a2a1248332c100924506ac8e6",
                    "name": "FVCK_CRYSTAL// #2319",
                    "token_id": "0x000000000000000000000000000000000000000000000000000000000000090f",
                    "media_uri": "https://arweave.net/sPXrw3NhMbdVEc1pfK-UPrTrzfBMilTqclBu-gDyM98"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0x84f6c4b892547a6acee98a3954bc2089f97c43f3",
                    "name": "Lil Brain #1352",
                    "token_id": "0x0000000000000000000000000000000000000000000000000000000000000547",
                    "media_uri": "https://ipfs.io/ipfs/QmT8YW4MgZy1Jz4rtqweFzcATpL2QRnK1E1j4wC1DYkgJ6"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0xa7d8d9ef8d8ce8992df33d8b8cf4aebabd5bd270",
                    "name": "Para Bellum  #10",
                    "token_id": "0x000000000000000000000000000000000000000000000000000000000f8e8b4a",
                    "media_uri": "https://media.artblocks.io/261000010.png"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0xa7d8d9ef8d8ce8992df33d8b8cf4aebabd5bd270",
                    "name": "Para Bellum  #819",
                    "token_id": "0x000000000000000000000000000000000000000000000000000000000f8e8e73",
                    "media_uri": "https://media.artblocks.io/261000819.png"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0xba30e5f9bb24caa003e9f2f0497ad287fdf95623",
                    "name": "",
                    "token_id": "0x0000000000000000000000000000000000000000000000000000000000000021",
                    "media_uri": "ipfs://QmU9WfhRD5FJ29N7ERCrrwRXCyP775Lq5GetmwydreNCP7"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0xbc4ca0eda7647a8ab7c2061c2e118a18a936f13d",
                    "name": "",
                    "token_id": "0x0000000000000000000000000000000000000000000000000000000000001e59",
                    "media_uri": "ipfs://QmUVxjmmecZLnHq7TDW2oYcX3QZPn5mZFkqFLv1WijavgQ"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0xc9301506425869dd79d50545481f48c07aa1ad25",
                    "name": "Dead Decor #100",
                    "token_id": "0x0000000000000000000000000000000000000000000000000000000000000064",
                    "media_uri": "https://deaddecor.co/storage/rooms/Jsw1SVeyM2SpNc1t3P91dzWVZtCFMYS6JTma26ZF.png"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0xda22422592ee3623c8d3c40fe0059cdecf30ca79",
                    "name": "",
                    "token_id": "0x00000000000000000000000000000000000000000000000000000000000031f7"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0xf70074f1cb0aa67917bbecf5476a6778e2056671",
                    "name": "Dead Heart",
                    "token_id": "0x00",
                    "media_uri": "ipfs://QmQKQFGb5NZm8TVHKYkK3rrdY3BAfaQH267iVif2jYeDUW"
                },
                "claim_status": "none"
                },
                {
                "token": {
                    "contract_address": "0xfdb3e529814afc5df5a5faf126989683b17daef9",
                    "name": "Doodlesaur #5442",
                    "token_id": "0x0000000000000000000000000000000000000000000000000000000000001542",
                    "media_uri": "https://storage.googleapis.com/doodlesaurs/images/p4wg8siorp6s.png"
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
    console.log(suiWalletAddress)
    await resolveAfter2Seconds();
    return"0x21fbf0696d5e0aa2ef41a2b4ffb623bcaf070461d61cf7251c74161f82fec3a4370854bc0a34b3ab487c1bc021cd318c734c51ae29374f2beb0e6f2dd49b4bf41c"
}


  

export const mintSuiNFTDemo = async (signature) => {
    console.log("mintSuiNFTDemo", signature)
     await resolveAfter2Seconds();

    const success = {
            "source_chain": "ethereum",
            "source_contract_address": "0xBC4CA0EdA7647A8aB7C2061c2E118A18a936f13D",
            "source_token_id": "101",
            "sui_explorer_link": "http:127.0.0.1:8000/BC4CA0EdA7647A8a"
        }

    try {

        /// ramdomly throw error! to test error handling
        if([true, false][Math.round(Math.random())]){
            throw new Error("Validation failed")
        }
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