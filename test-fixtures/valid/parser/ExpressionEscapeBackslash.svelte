<script lang="ts">
	function open(path: string) {}
</script>

<!-- String ending with \\ before closing quote; the naive single-char lookbehind
     incorrectly treats the closing quote as escaped -->
<button onclick={() => open('C:\\Users\\')}>Open</button>

<!-- Deeper nesting: UNC-style path ending with \\ -->
<button onclick={() => open('\\\\Server\\Share\\')}>UNC</button>
