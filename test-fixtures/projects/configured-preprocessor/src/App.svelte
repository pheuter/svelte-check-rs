<script lang="ts">
	import Child from './Child.svelte';

	/* 😀 */ const answer: number = "__PREPROCESS_TO_NUMBER__";
</script>

<h1>Configured preprocessor</h1>
<span>😀</span><h3>Processed heading</h3>
<p>{answer}</p>
<span>😀</span><img src="fixture.png" />
<Child />
