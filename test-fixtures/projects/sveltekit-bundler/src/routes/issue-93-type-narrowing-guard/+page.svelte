<script lang="ts">
  // Issue #93 variant: Type narrowing with type guards
  // This tests that type guard functions work correctly

  type Result<T> = { ok: true; value: T } | { ok: false; error: string };

  function isOk<T>(result: Result<T>): result is { ok: true; value: T } {
    return result.ok;
  }

  // Use a function to get result so TypeScript can't statically narrow it
  function getResult(): Result<string> {
    return Math.random() > 0.5
      ? { ok: true, value: "Hello" }
      : { ok: false, error: "Failed" };
  }

  const result = getResult();

  // Type narrowing via type guard
  if (!isOk(result)) {
    throw new Error(result.error);
  }

  // After guard, result should be { ok: true; value: string }
  const message = result.value;
</script>

<!-- Template should see result.value as string -->
<p>{result.value}</p>
<p>Message: {message}</p>
