<script lang="ts">
    import NftImage from "./NftImage.svelte"
    import { createEventDispatcher } from "svelte"
    import {walletAddress } from '../store'
    import { startMinting, startSigning } from '../store/ApiEndPoints'

    import Loader from "./Loader.svelte"
    import Error from "./Error.svelte"
    import BgOject from "./BgOject.svelte"

    export let data:any;
    let suiWalletAddress:string = ""

    // handle error state
    $:error = false
    let activeSections = 'addwallet'



    const dispatch = createEventDispatcher()

    const changeSelectedNFT = () => {
      dispatch('selectNFT', false)
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
            let signObj = {
                message: {
                    sui_address: suiwalledAdr,
                    eth_contract_address: data.eth_contract_address,
                    token_id: data.token_id,
                    minter:$walletAddress,
                    timestamp: Date.now()
                }
            } 
            return startSigning(signObj)
        } catch (err) {
            error  =  err.message
            throw err
        }
    }
    const mintSuiNFTFn = async (signature:string) => {
        console.log(signature)
        try {
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
                                        <div class="checkmark">
                                            <svg version="1.1" id="Layer_1" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" x="0px" y="0px"
                                               viewBox="0 0 161.2 161.2" enable-background="new 0 0 161.2 161.2" xml:space="preserve">
                                                  <circle class="circle" fill="none" stroke="#FFFFFF" stroke-width="7" stroke-miterlimit="10" cx="80.6" cy="80.6" r="62.1"></circle>
                                                  <polyline class="icon" fill="none" stroke="#FFFFFF" stroke-width="7" stroke-linecap="round" stroke-miterlimit="10" points="113,52.8 74.1,108.4 48.2,86.4"></polyline>
                                              </svg>
                                        </div>
                                        <h2 class="title"> Minted on Sui network</h2>
                                        <a href="{response.sui_explorer_link}" target="_blank" class="axil-btn btn-large btn-fill-white">View in Explorer</a>
                                        <button  class="axil-btn btn-fill-white btn-large err-btn" on:click="{changeSelectedNFT}">Mint another NFT</button>
                                    </div>
                                </div>
                            {:catch err}
                                <Error errmessage={`${err.message} - ${err.details.requestBody.message}`} /> 
                                <button  class="axil-btn btn-fill-white btn-large err-btn" on:click="{changeSelectedNFT}">Select another NFT</button>
                            {/await}  

                    {:catch err}
                    <div class="section section-padding bg-color-light pb--70 error ">
                        <Error errmessage={err.message} />     
                        <button  class="axil-btn btn-fill-white btn-large" >Change Address </button>
                    </div>
                {/await}


            </div>
        {/if}
        <BgOject />
    </div>
    <BgOject />
</div>
<style lang="scss">
    @import "../styles/app.scss";
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

    .checkmark {
  width: 100px;
  margin: 0 auto;
  padding-top: 20px;
}

.circle {
  -moz-animation-name: circle-animation;
  -webkit-animation-name: circle-animation;
  animation-name: circle-animation;
  -moz-animation-duration: 2s;
  -webkit-animation-duration: 2s;
  animation-duration: 2s;
  -moz-animation-timing-function: ease-in-out;
  -webkit-animation-timing-function: ease-in-out;
  animation-timing-function: ease-in-out;
  stroke-dasharray: 1000;
  stroke-dashoffset: 0;
}

.icon {
  -moz-animation-name: icon-animation;
  -webkit-animation-name: icon-animation;
  animation-name: icon-animation;
  -moz-animation-duration: 1s;
  -webkit-animation-duration: 1s;
  animation-duration: 1s;
  -moz-animation-timing-function: ease-in-out;
  -webkit-animation-timing-function: ease-in-out;
  animation-timing-function: ease-in-out;
  opacity: 1;
}

@keyframes circle-animation {
  0% {
    stroke-dashoffset: 1000;
  }
  100% {
    stroke-dashoffset: 0;
  }
}
@keyframes icon-animation {
  0% {
    opacity: 0;
  }
  50% {
    opacity: 0;
  }
  100% {
    opacity: 1;
  }
}

</style>