<script lang="ts">
    // Issue #20: svelte-ignore pragma should suppress warnings
    let value = $state('test');
</script>

<!-- svelte-ignore a11y-no-noninteractive-tabindex -->
<div tabindex="0">
    This div has tabindex but the warning should be suppressed
</div>

<p>{value}</p>
