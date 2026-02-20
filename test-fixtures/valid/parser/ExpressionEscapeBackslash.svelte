<script lang="ts">
	function open(path: string) {}
</script>

<!-- String ending with \\ before closing quote -->
<button onclick={() => open('C:\\Users\\')}>Open</button>

<!-- UNC-style path ending with \\ -->
<button onclick={() => open('\\\\Server\\Share\\')}>UNC</button>

<!-- Template literal with escaped backslash -->
<button onclick={() => open(`path\\`)}>Template</button>
