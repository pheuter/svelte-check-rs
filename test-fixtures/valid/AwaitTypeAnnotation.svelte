<script lang="ts">
    type Props = { a: number; b: string };
    let p: Promise<Props> = Promise.resolve({ a: 1, b: 'x' });
</script>

<!-- Plain binding -->
{#await p then v}
    <p>{v.a}</p>
{/await}

<!-- Type-annotated then/catch bindings -->
{#await p}
    <p>loading</p>
{:then v: Props}
    <p>{v.a} {v.b}</p>
{:catch e: unknown}
    <p>{String(e)}</p>
{/await}

<!-- Destructured then binding -->
{#await p then { a, b }}
    <p>{a} {b}</p>
{/await}

<!-- Destructured + type-annotated then binding -->
{#await p then { a, b }: Props}
    <p>{a} {b}</p>
{/await}
