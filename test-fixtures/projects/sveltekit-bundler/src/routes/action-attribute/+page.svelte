<script lang="ts">
  import type { ActionReturn } from 'svelte/action';

  type Params = { options: { enabled: boolean } };

  function testAction(_node: HTMLElement, _params: Params): ActionReturn {
    return {};
  }
</script>

<div use:testAction={{ option: { enabled: true } }}></div>
