<script lang="ts">
  let count = $state(0)

  export const snapshot = {
    capture: () => ({ count }),
    restore: (values: { count: number }) => (count = values.count),
  }
</script>

<button onclick={() => count++}>Count: {count}</button>
