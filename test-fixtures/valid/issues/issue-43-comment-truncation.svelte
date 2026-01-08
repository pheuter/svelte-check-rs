<script lang="ts">
  let count = $state(0);

  $effect(() => {
    // Flow explanation:
    // 1. Something happens
    // 2. Variable changes -> effect re-runs
    // 3. Cleanup runs
    // 4. Effect body checks someVar=true, returns early
    // 5. Operation fails

    console.log(count);
  });
</script>

<button onclick={() => count++}>
  Count: {count}
</button>
