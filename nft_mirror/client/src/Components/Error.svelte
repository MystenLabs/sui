<script lang="ts">
    export let errmessage:boolean | string = false
</script>

{#if errmessage}
    <p class="error">{errmessage}</p>
{/if}

<style lang="scss">
    .error {
        margin-top: 20px;
        margin-bottom: 20px;
        font-weight: 600;
        color: #FFF;
        text-transform: uppercase;
    }
</style>