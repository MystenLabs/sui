<script lang="ts">
  export let state: boolean = false;
  export let blueloading: boolean = false;
</script>

{#if state}
  <div class="loading {blueloading ? 'blueloading' : ''}">
    <div class="line" />
    <div class="line" />
    <div class="line" />
  </div>
{/if}

<style lang="scss">
  @keyframes loadingB {
    0% {
      width: 10%;
    }
    50% {
      width: 30%;
    }
    100% {
      width: 10%;
    }
  }
  .line {
    display: inline-block;
    width: 10%;
    height: 10px;
    border-radius: 15px;
    background-color: #fefefe;
    margin: auto;
  }
  .blueloading .line {
    background-color: #6fbcf0;
  }

  .loading .line:nth-last-child(1) {
    animation: loadingB 1.5s 1s infinite;
  }
  .loading .line:nth-last-child(2) {
    animation: loadingB 1.5s 0.5s infinite;
  }
  .loading .line:nth-last-child(3) {
    animation: loadingB 1.5s 0s infinite;
  }
</style>
