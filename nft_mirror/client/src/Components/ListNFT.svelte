<script lang="ts">
    import { createEventDispatcher } from "svelte"
    import { fetchNFTDataByAddress } from '../store/ApiEndPoints'
    import { walletAddress } from '../store'

    import Loader from "./Loader.svelte"
    import Error from "./Error.svelte"
    import NftImage from "./NftImage.svelte"
    import BgOject from "./BgOject.svelte"
   
    import AddSuiWallet from './AddSuiWallet.svelte';

   
    export let address:any = $walletAddress
    $:selectedNFT = null

    const dispatch = createEventDispatcher();

    const enterSuiAddress = (nftObj:string) => {
        selectedNFT = nftObj
    }

    const selectAnotherNFT = () => {
        selectedNFT = false;
    }

    let nft_list:any = []
    const getNFTDataByAddress = async (address:string) => {
        try {
            /// chack if the list is empty or not return the list
            if(nft_list.length > 0) {
                return nft_list
            }
            nft_list = await fetchNFTDataByAddress(address)
            return nft_list
        } catch (error) {
            throw error
        }
    }

    const switchWalletAddress = () => {
        $walletAddress = ''
        nft_list = []
        dispatch('pageFn', {page:'landing'});
    }

</script>

<section class="section section-padding-2 _sui-nfts-list">
    <div class="_sui-nfts">
        <div class="container">
            {#if !selectedNFT}
                <div class="axil-isotope-wrapper">
                    {#await getNFTDataByAddress(address)}
                        Loading <Loader state={true} blueloading={true}/>
                        {:then item}
                            {#if item.length}
                                <div class="section-heading heading-left mb--40">
                                    <h2 class="title">Select Token to mint on Sui</h2>
                                </div>
                                <div class="row row-35 isotope-list">
                                    {#each item as itm, ir (ir)}
                                        <div class="col-md-3 project _nft-item {itm.claim_status === 'none' ? '' : 'claimed'}">
                                            <!-- svelte-ignore a11y-invalid-attribute -->
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
                            <button  class="axil-btn btn-fill-white btn-large" on:click="{switchWalletAddress}">Change Address <img class="metamask-logo" src="assets/logos/metamask-fox.svg" alt="metamask logo" /> </button>
                        {:catch err}
                        <div class="section section-padding bg-color-light pb--70 error ">
                            <Error errmessage={err.message} />     
                            <button  class="axil-btn btn-fill-white btn-large" on:click="{switchWalletAddress}">Change Address <img class="metamask-logo" src="assets/logos/metamask-fox.svg" alt="metamask logo"/> </button>
                            <BgOject />
                        </div>
                    {/await}
                </div>
            {/if}  
            {#if selectedNFT}        
                <AddSuiWallet data={selectedNFT} on:selectNFT={selectAnotherNFT}  on:changeWalletAddr={switchWalletAddress}/>
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
    @import "../styles/variables.scss";
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
        ._nft-item{
            cursor: pointer;
        }
        
        
    }
    .error{
        background-color: $sui__blue;
        width: 1200px;
        margin: 0 auto;
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
    :global(.claimedNFT){
        font-size: 12px;
        font-weight: 600;
        color: #900202;
     }
</style>