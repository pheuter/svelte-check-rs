// Param matcher for parity tests.  The matcher body is a chain of equality
// comparisons; TypeScript 5.5+ infers the return type as
// `(param: string) => param is "alpha" | "beta" | "gamma"`, and SvelteKit's
// generated `MatcherParam<typeof match>` uses that predicate to narrow the
// route parameter type in `RouteParams`.
//
// svelte-check-rs's params transform used to add an explicit `: boolean`
// return annotation that defeated the inferred predicate.  This file
// exists so the parity integration test can confirm the narrowing flows
// all the way to `+page.svelte` consumers — see
// `src/routes/issue-parity-matcher/[id=restricted]/+page.svelte`.
export const match = (param: string) =>
  param === 'alpha' || param === 'beta' || param === 'gamma';
