<script>
  import MyComponent from './MyComponent.svelte'
</script>

<!-- Issue #37: CSS custom property attributes -->
<MyComponent --primary-color="red" />
<MyComponent --primary-color="blue" --secondary-color="green" />

<!-- Additional test cases -->
<div style:--custom-var="value">
  Custom styled content
</div>
