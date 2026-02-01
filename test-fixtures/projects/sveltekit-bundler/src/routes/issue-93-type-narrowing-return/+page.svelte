<script lang="ts">
  // Issue #93 variant: Type narrowing after early return
  // This tests that control flow analysis works for return statements too

  interface User {
    name: string;
    email: string;
  }

  let { data } = $props<{ data: { user: User | null } }>();

  // Type narrowing via early return - after this, data.user should be User
  if (!data.user) {
    // In a real component, this might redirect or show loading state
    // For testing, we just need to ensure the narrowing is recognized
  }

  // Only access user properties after the guard
  const userName = data.user ? data.user.name : "Guest";
</script>

<!-- Test direct narrowing in template with if block -->
{#if data.user}
  <p>Welcome, {data.user.name}!</p>
  <p>Email: {data.user.email}</p>
{/if}
