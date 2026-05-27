<script lang="ts">
  // Issue #149: getter/setter (function-binding) form of `bind:this`.
  let inputRef = $state<HTMLInputElement | null>(null);

  function setInputRef(el: HTMLInputElement) {
    inputRef = el;
  }
</script>

<input bind:this={() => inputRef, setInputRef} />
