<script lang="ts">
    import type { PageData } from './$types';
    import Button from '$lib/components/Button.svelte';

    let { data }: { data: PageData } = $props();

    // VALID: accessing typed data from page load
    const posts = data.posts;
    const totalCount = data.totalCount;

    // ERROR: 'comments' doesn't exist on PageData
    const comments = data.comments;
</script>

<h1>Posts ({totalCount})</h1>

{#each posts as post}
    <article>
        <h3>{post.title}</h3>
        <p>{post.content}</p>
        <!-- ERROR: 'author' doesn't exist on post -->
        <span>By: {post.author}</span>
    </article>
{/each}

<!-- VALID: correct props -->
<Button variant="primary" onclick={() => console.log('clicked')}>
    Click me
</Button>

<!-- ERROR: 'wrong' is not a valid prop on Button -->
<Button variant="primary" wrong>
    Invalid Prop
</Button>

<!-- ERROR: 'invalid' is not a valid variant -->
<Button variant="invalid">
    Invalid Variant
</Button>
