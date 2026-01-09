<script lang="ts">
  // Issue #46: Regex edge cases - quantifiers, escapes, char classes, etc.
  let value = $state("test123");
  let items = $state(["a", "b", "c"]);
</script>

<!-- Regex with quantifier braces {n,m} -->
{value.match(/\d{2,4}/)}

<!-- Regex with escape sequences -->
{value.match(/\n\t\r\s\w/)}

<!-- Regex with character class containing } -->
{value.match(/[{}]+/)}

<!-- Regex with character class containing ) -->
{value.match(/[^)]+/)}

<!-- Regex with flags -->
{value.replace(/test/gi, "REPLACED")}

<!-- Regex with escaped slash -->
{value.match(/path\/to\/file/)}

<!-- Regex in array -->
{[/a/, /b/, /c/]}

<!-- Regex in object -->
{{ pattern: /test/, flags: "gi" }}

<!-- Regex after ternary operator -->
{true ? /yes/ : /no/}

<!-- Regex after logical operators -->
{false || /fallback/.test(value)}

<!-- Regex in arrow function filter -->
{#each items.filter(x => /test/.test(x)) as item}
  <span>{item}</span>
{/each}

<!-- Regex in template literal -->
{`result: ${/test/.test(value)}`}

<!-- Division (should NOT be treated as regex) -->
{(10 + 5) / 3}
{value.length / 2}
