<script>
  let value = $state('');
  let selected = $state('');
  let checked = $state(false);
</script>

<!-- Components named after HTML void elements should work with explicit closing tags -->
<Input bind:value={value} placeholder="Enter text"></Input>

<Input type="radio" name="group" value="option1" bind:group={selected}>
  {#snippet label()}
    Option 1
  {/snippet}
</Input>

<Input type="checkbox" bind:checked={checked}>
  {#snippet label()}
    Accept terms
  {/snippet}
</Input>

<!-- Self-closing should also work -->
<Input bind:value={value} />
<Img src="photo.jpg" alt="Photo" />
<Link href="/about" />
<Source srcset="image.webp" />

<!-- Nested inside another component -->
<Form>
  <Input bind:value={value}></Input>
</Form>
