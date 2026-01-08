<script lang="ts">
  let count = $state(0);
  let thisBarIsDragging = $state(false);

  $effect(() => {
    // Flow explanation:
    // 1. Something happens
    // 2. Variable changes -> effect re-runs
    // 3. Cleanup runs FIRST -> destroys gesture mid-drag!
    // 4. Effect body checks thisBarIsDragging=true, returns early
    // 5. Operation fails

    console.log(count);
  });
</script>

<button onclick={() => count++}>
  Count: {count}
</button>
