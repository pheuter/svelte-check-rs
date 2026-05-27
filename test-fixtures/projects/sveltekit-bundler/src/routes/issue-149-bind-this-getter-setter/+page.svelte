<script lang="ts">
  // Issue #149: the getter/setter (function-binding) form of `bind:this`.
  // https://svelte.dev/docs/svelte/bind#Function-bindings
  let inputRef = $state<HTMLInputElement | null>(null);

  function setInputRef(el: HTMLInputElement) {
    inputRef = el;
  }

  function focus() {
    inputRef?.focus();
  }
</script>

<input bind:this={() => inputRef, setInputRef} placeholder="Type here..." />
<button onclick={focus}>Focus input</button>
