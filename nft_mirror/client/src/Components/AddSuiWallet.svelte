<script lang="ts">
    import NftImage from "./NftImage.svelte"
    import { createEventDispatcher } from "svelte"
    import { walletAddress } from '../store'
    import { startMinting, startSigning } from '../store/ApiEndPoints'

    import Loader from "./Loader.svelte"
    import Error from "./Error.svelte"
    import BgOject from "./BgOject.svelte"
    import ResponseCheckMark from "./ResponseCheckMark.svelte";

    export let data:any;
    let suiWalletAddress:string = ""

    // handle error state
    $:error = false
    let activeSections = 'addwallet'



    const dispatch = createEventDispatcher()

    const changeSelectedNFT = () => {
      dispatch('selectNFT', false)
    }

    const changeWalletAddress = () => {
      dispatch('changeWalletAddr', false)
    }

    const signSignatureAndMint = () => {
        error = false
      /// add wallllet address validation
      if(!suiWalletAddress)  {
          error = true
          return
      }
      activeSections= 'mint'
    };
    
    /**
     *
     * @param {string} walletAddress
     */
    // {sui_address:xxx, eth_contract_address, token_id:xx, minter, timestamp}


    const signNTFSign = async (suiwalledAdr:string) => {
        try {
            /// format the signature
            // {sui_address:xxx, eth_contract_address, token_id:xx, minter, timestamp}
            let signObj =  {
                domain: {
                    // Ethereum Mainnet
                    chainId: 1,
                    // Give a user friendly name to the specific contract you are signing for.
                    name: 'SuiDrop',
                    // Just let's you know the latest version
                    version: '1',
                },

                message: {
                    source_chain: "ethereum",
                    source_contract_address: data.contract_address,
                    source_token_id: '8937',
                    source_owner_address: $walletAddress,
                    destination_sui_address: '0xa5e6dbcf33730ace6ec8b400ff4788c1f150ff7e'   
                 },
                // Refers to the keys of the *types* object below.
                primaryType: 'ClaimRequest',
                types: {
                    // TODO: Clarify if EIP712Domain refers to the domain the contract is hosted on
                    EIP712Domain: [
                        { name: 'name', type: 'string' },
                        { name: 'version', type: 'string' },
                        { name: 'chainId', type: 'uint256' }
                    ],
                    
                    // Refer to PrimaryType
                    ClaimRequest: [
                        { name: 'source_chain', type: 'string' },
                        { name: 'source_contract_address', type: 'string' },
                        { name: 'source_token_id', type: 'string' },
                        { name: 'source_owner_address', type: 'string' },
                        { name: 'destination_sui_address', type: 'string' }
                    ],
                },
                 
                
            }
            const params = [$walletAddress, signObj];
            console.log(signObj)
            return startSigning($walletAddress, JSON.stringify(signObj))
        } catch (err) {
            error  =  err.message
            throw err
        }
    }
    const mintSuiNFTFn = async (signature:string) => {


        try {
            /// TODO: martch data to the format 
            const reqObj = {
                "message": {
                    "source_chain": "ethereum",
                    "source_contract_address": data.contract_address,
                    "source_token_id": data.token_id,
                    "source_owner_address": "0x529f501ceb3ab599274a38f2aee41a7eba1fcead",
                    "destination_sui_address": "0x10"
                },
                "signature": signature
            }
            return startMinting(reqObj)
        } catch (err) {
            error  =  err.message
            throw err
        }
    }


</script>
<div class="section bg-color-blue">
    <div class="sui-wallet">
        {#if activeSections === 'addwallet'}
            <div class="sui-content ">
                <div class="row row-35 isotope-list">
                    <div class="col-md-3 project _nft-item sui-nftImg {data.claim_status === 'none'  ? '' : 'claimed'}">
                        <NftImage nftData={data} />
                    </div>
                </div>
                {#if error}
                    <span class="validation-hint">
                        INVALID - Sui wallet required
                    </span>
                {/if}
                <input type="text" class="form-control" placeholder="Enter Sui wallet address" bind:value="{suiWalletAddress}" >
                <div class="row row-35 _cta">
                    <button class="axil-btn btn-fill-white btn-large" on:click="{changeSelectedNFT}">Change NFT</button>
                    <button class="axil-btn btn-fill-white btn-large" on:click="{signSignatureAndMint}">Mint NFT</button>      
                </div>
            </div>
        {/if}
        {#if activeSections === 'mint'}
            <div class="sui-content ">
                {#await signNTFSign(suiWalletAddress)}
                    <h3 class="title">Signing Address</h3>
                     <Loader state={true} />
                    {:then signature}
                        {#await mintSuiNFTFn(signature)}
                            <h3 class="title">Minting on Sui</h3> 
                            <Loader state={true} />
                            {:then response}
                                <div class="container">
                                    <div class="section-heading heading-light">
                                        <ResponseCheckMark responsetype="success" />
                                        <h2 class="title">Minted on Sui network</h2>
                                        <a href="{response.sui_explorer_link}" target="_blank" class="axil-btn btn-large btn-fill-white">View in Explorer</a>
                                        <button  class="axil-btn btn-fill-white btn-large err-btn" on:click="{changeSelectedNFT}">Mint another NFT</button>
                                    </div>
                                </div>
                            {:catch err}
                                <ResponseCheckMark responsetype="error" />
                                <Error errmessage={`${err.message} - ${err.details.requestBody.message}`} /> 
                                <button  class="axil-btn btn-fill-white btn-large err-btn" on:click="{changeSelectedNFT}">Select another NFT</button>
                            {/await}  
                    {:catch err}
                    <div class="error ">
                        <ResponseCheckMark responsetype="error" />
                        <Error errmessage={err.message} />     
                        <button  class="axil-btn btn-fill-white btn-large" on:click="{changeWalletAddress}">Change Address </button>
                    </div>
                {/await}
            </div>
        {/if}
        <BgOject />
    </div>
    <BgOject />
</div>
<style lang="scss">
    // @import "../styles/app.scss";
    @import "../styles/variables.scss";
    .section{
        display: flex;
        justify-content: center;
        align-items: center;
        border-radius: 20px;
    }
    .sui-wallet{
        max-width: 1200px;
        width: 100%;
        margin: 0 auto;
        display: flex;
        flex-direction: column;
        justify-content: center;
        .sui-content{
            height: 600px;
            display: flex;
            flex-direction: column;
            justify-content: center;
        }

       input{
           max-width: 60%;
           margin: auto;
           margin: 0 auto;
           font-weight: 600;
           text-align: center;
           background-color: $sui__white;
       } 
       .title{
           color: $sui__white;
           font-size: 40px;
           font-weight: 400;
            margin-top: 0;
           text-align: center;
       }
     ._nft-item{
         margin-bottom: 30px;
     }
       ._cta{
            margin: 0 auto;
            margin-top: 20px;
           .axil-btn{
               width: 300px;
           }
       }
    }
    .axil-btn {
        padding: 12px 45px;
        margin: 5px;
        color:$sui__blue;
        @media only screen and (max-width: 767px) {
            margin: 5px auto;
        }
    }
    .validation-hint{
        color: #FFF;
        font-size: 16px;
        margin-top: 10px;
        text-align: center;
        font-weight: 600;
        text-transform: uppercase;
    }
    .err-btn{
        width: 300px;
        margin: 0 auto;
    }


</style>