<script lang="ts">
    // Issue #21: Regex literals with colons should not confuse the parser
    const timeRegex = /\d{2}:\d{2}:\d{2}/;
    const urlRegex = /https?:\/\/[^\s]+/;

    let value = $state('12:30:45');

    function isValidTime(str: string): boolean {
        return timeRegex.test(str);
    }
</script>

<svelte:head>
    <title>Regex Test</title>
</svelte:head>

<p>{isValidTime(value) ? 'Valid' : 'Invalid'}</p>
