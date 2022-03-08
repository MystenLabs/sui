<script lang="ts">
    import { createEventDispatcher } from "svelte"
    import { fetchNFTDataByAddress } from '../store/ApiEndPoints'
    import Loader from "./Loader.svelte"
    import Error from "./Error.svelte"
    import NftImage from "./NftImage.svelte"

    import { walletAddress } from '../store'
    import AddSuiWallet from './AddSuiWallet.svelte';

   
    export let address:any = $walletAddress
    $:selectedNFT = null

    const dispatch = createEventDispatcher();

    const enterSuiAddress = (nftObj:string) => {
        selectedNFT = nftObj
    }

    const selectAnotherNFT = (data: any) => {
        console.log('called')
        selectedNFT = false;
   };

    const switchWalletAddress = () => {
        $walletAddress = ''
        dispatch('pageFn', {page:'landing'});
    }

</script>

<section class="section section-padding-2 _sui-nfts-list">
    <div class="_sui-nfts">
        <div class="container">
            {#if !selectedNFT}
                <div class="axil-isotope-wrapper">
                    {#await fetchNFTDataByAddress(address)}
                        Loading <Loader state={true} blueloading={true}/>
                        {:then item}
                            {#if item.length}
                                <div class="section-heading heading-left mb--40">
                                    <span class="subtitle">Token on Address</span>
                                    <h2 class="title">Select Token to mint on Sui</h2>
                                </div>
                                <div class="row row-35 isotope-list">
                                    {#each item as itm, ir (ir)}
                                        <div class="col-md-3 project _nft-item {itm.claim_status === 'none' ? '' : 'claimed'}">
                                            <a href="javascript:void(0)" on:click={()=>enterSuiAddress(itm)}>
                                                <NftImage nftData="{itm}" />
                                            </a>
                                        </div>
                                    {/each}
                                </div>
                            {/if}
                            {#if !item.length}
                                <div class="section-heading heading-left mb--40">
                                    <h2 class="title text-center">No NFT found on this address</h2>
                                </div>
                            {/if}
                            <button  class="axil-btn btn-fill-white btn-large" on:click="{switchWalletAddress}">Change Address <img class="metamask-logo" src="assets/logos/metamask-fox.svg" /> </button>
                        {:catch err}
                        <div class="section section-padding bg-color-light pb--70 error ">
                            <Error errmessage={err.message} />     
                            <button  class="axil-btn btn-fill-white btn-large" on:click="{switchWalletAddress}">Change Address <img class="metamask-logo" src="assets/logos/metamask-fox.svg" /> </button>
                            <ul class="list-unstyled shape-group-9">
                                <li class="shape shape-1"><img src="assets/bubble-12.png" alt="Shapes"></li>
            
                                <li class="shape shape-2"><img src="assets/bubble-16.png" alt="Comments"></li>
                                <li class="shape shape-3"><img src="assets/bubble-13.png" alt="Comments"></li>
                                <li class="shape shape-4"><img src="assets/bubble-14.png" alt="Comments"></li>
                                <li class="shape shape-5"><img src="assets/bubble-16.png" alt="Comments"></li>
                                <li class="shape shape-6"><img src="assets/bubble-15.png" alt="Comments"></li>
                                <li class="shape shape-7"><img src="assets/bubble-16.png" alt="Comments"></li>
                            </ul>
                        </div>
                    {/await}
                </div>
            {/if}  
            {#if selectedNFT}        
                <AddSuiWallet data={selectedNFT} on:selectNFT={ selectAnotherNFT} />
            {/if}
        </div>
        <ul class="shape-group-7 list-unstyled">
            <li class="shape shape-1"><img src="./../assets/background/circle-2.png" alt="circle"></li>
            <li class="shape shape-2"><img src="./../assets/background/bubble-2.png" alt="Line"></li>
            <li class="shape shape-3"><img src="./../assets/background/bubble-1.png" alt="Line"></li>
        </ul>
    </div>
</section>

<style lang="scss">
    @import "../styles/app.scss";

    ._sui-nfts{
        .container{
            max-width: 1260px;
        }
    }
    ._sui-nfts-list{
        text-align: center;
        display: flex;
        flex-direction: column;
        justify-content: center;
        min-height: 85vh;
        h2.title{
            font-size: 40px;
        }
        h4.title{
            font-size: 20px;
            color:$sui__black;
        }
        ._nft-item{
            cursor: pointer;
            .content { 
                background-color: #f7f7f7;
            }
        }
        
        .claimedNFT{
            font-size: 12px;
            font-weight: 600;
            color: #900202;
        }
    }
    .error{
        background-color: $sui__blue;
        width: 1200px;
        margin: 0 auto;
        p{
            color: #900202 !important;
        }
        text-align: center;
    }
    .axil-btn {
        padding: 8px 45px;
    }
    .axil-isotope-wrapper{
        .axil-btn{
            background-color: $sui__blue;
            color:#E1F3FF
        }
    }
</style>