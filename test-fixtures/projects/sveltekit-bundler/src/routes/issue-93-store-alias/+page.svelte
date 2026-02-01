<script lang="ts">
  import { writable } from 'svelte/store';

  const form = { form: writable({ value: 1 }) };
  const { form: formData } = form;
</script>

<p>{$formData.value}</p>
