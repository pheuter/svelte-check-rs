<script lang="ts">
    // Issue #21: Imports with colons (like virtual:pwa-register) should not
    // cause parsing errors on Svelte special elements like <svelte:head>.
    const virtualImport = 'virtual:pwa-register';
    const anotherColon = "some:value:here";

    let count = $state(0);
</script>

<svelte:head>
    <title>Test Page</title>
</svelte:head>

<button onclick={() => count++}>
    Count: {count}
</button>

<style>
    :global(body) {
        margin: 0;
    }
</style>
