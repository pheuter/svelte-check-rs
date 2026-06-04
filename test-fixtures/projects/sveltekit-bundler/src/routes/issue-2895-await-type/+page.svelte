<script lang="ts">
    // Issue #2895: a user-supplied type annotation on the await `then`/`catch`
    // binding must be preserved (and checked against the awaited type) rather than
    // clobbered by a forced annotation, which produced syntactically broken TS
    // like `const v: { name: string }: Awaited<...> = ...`.
    let p: Promise<{ name: string }> = Promise.resolve({ name: 'svelte' });
</script>

{#await p}
    <p>loading</p>
{:then v: { name: string }}
    <p>{v.name}</p>
{:catch e: Error}
    <p>{e.message}</p>
{/await}
