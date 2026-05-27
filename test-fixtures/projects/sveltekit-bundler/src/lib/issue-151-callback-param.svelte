<script lang="ts">
  // Issue #151, pattern B: dollar-prefixed callback parameters are not stores.
  // The $item / $val params reference bare item / val, which are never declared,
  // so no spurious store alias must be emitted for them. A real store (form,
  // declared bare) still auto-subscribes as $form.
  import { writable } from "svelte/store";

  interface FormData {
    name: string;
    location: string;
  }

  const form = writable<FormData>({ name: "", location: "" });

  function updateLocation(newLoc: string) {
    form.update(($form) => ({ ...$form, location: newLoc }));
  }

  const items = [1, 2, 3];
  const mapped = items.map(($item) => $item * 2);

  function process(cb: ($val: number) => number) {
    return cb(42);
  }
  const result = process(($val) => $val + 1);
</script>

<button onclick={() => updateLocation("NYC")}>Update</button>
<p>{mapped.join(", ")} / {result}</p>
