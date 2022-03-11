<script lang="ts">
  export let nftData;
</script>


    <div class="project-grid _nft-img">
      <div class="thumbnail">
        <img src={nftData.media_uri} alt={nftData.name} />
      </div>
      <div class="content">
        <h4 class="title ">{nftData.name}</h4>
        {#if nftData.claim_status !== "none"}
          <div>
            <span class="claimedNFT">Claimed </span>
          </div>
        {/if}
      </div>
    </div>
 


<style lang="scss">
    @import "../styles/variables.scss";
    ._nft-img {
      max-width: 260px;
        margin: auto;
        .thumbnail {
          
            img {
                object-fit: cover;
               // max-height: 250px;
            }
        }
        .content {
            background-color: #f7f7f7;
            padding: 20px;
            text-align: left;
            .title {
                font-size: 20px;
                overflow-wrap: break-word;
                word-wrap: break-word;
                hyphens: auto;
                color: $sui__black;
            }
        }

        @media only screen and (max-width: 767px) {
            width: 80%;
            margin: auto;
        }
    }
    :global( ._nft-item .thumbnail){
        margin: auto;
        max-width: 16vw;
       
    }
</style>
