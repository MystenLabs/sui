<script lang="ts">
  export let responsetype: string = "success";
</script>

<div class="checkmark">
  <svg
    version="1.1"
    id="Layer_1"
    xmlns="http://www.w3.org/2000/svg"
    xmlns:xlink="http://www.w3.org/1999/xlink"
    x="0px"
    y="0px"
    viewBox="0 0 161.2 161.2"
    enable-background="new 0 0 161.2 161.2"
    xml:space="preserve"
  >
    <circle
      class="circle"
      fill="none"
      stroke="#FFFFFF"
      stroke-width="7"
      stroke-miterlimit="10"
      cx="80.6"
      cy="80.6"
      r="62.1"
    />
    {#if responsetype === "success"}
      <polyline
        class="icon"
        fill="none"
        stroke="#FFFFFF"
        stroke-width="7"
        stroke-linecap="round"
        stroke-miterlimit="10"
        points="113,52.8 74.1,108.4 48.2,86.4"
      />
    {:else}
      <path
        class="icon"
        fill="none"
        stroke="#FFFFFF"
        stroke-width="7"
        stroke-miterlimit="10"
        d="M 112,112 L 50,50 M 50,112 L 112,50"
      />
    {/if}
  </svg>
</div>

<style lang="scss">
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
