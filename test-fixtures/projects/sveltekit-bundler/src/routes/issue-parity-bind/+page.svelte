<script lang="ts">
  // Parity fixture: `bind:value={expr}` on a component used to emit
  // `value: undefined as any` in the generated TypeScript, which silently
  // dropped the user's expression type.  Now the bound expression flows
  // through the prop slot, so the write-direction mismatch is caught.
  import BindTarget from './BindTarget.svelte';

  let nullable: string | null = $state(null);
</script>

<!-- `nullable` is `string | null` but BindTarget.value is `string | undefined`. -->
<!-- TypeScript must report TS2322 ("null not assignable to undefined"). -->
<BindTarget bind:value={nullable as string | null} />
