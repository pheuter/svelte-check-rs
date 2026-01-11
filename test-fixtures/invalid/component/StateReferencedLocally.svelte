<!-- Component validation: state_referenced_locally -->
<script lang="ts">
  let { data } = $props();

  // This should warn: "This reference only captures the initial value of `data`"
  const { form } = data;

  // This should also warn
  let items = $state(data.items);
</script>

<p>Form: {form}</p>
