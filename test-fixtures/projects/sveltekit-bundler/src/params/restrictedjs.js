// Checked `.js` counterpart of `restricted.ts` (the project's tsconfig enables
// `allowJs` + `checkJs`, so this file is type-checked).  It exists to pin the
// #2939 fix: the params transform must NOT leak the TS-only `satisfies`
// operator into a `.js` file — doing so produced a false-positive TS8010/
// TS8037.  The trailing `ParamMatcher` constraint is now emitted as a JSDoc
// `@satisfies` cast for `.js`, which preserves the TS 5.5+ inferred type
// predicate identically while staying valid JavaScript.
//
// The matcher has NO type annotations and NO leading JSDoc, so the transform
// injects both a JSDoc `@param {string}` AND the trailing JSDoc `@satisfies`
// cast.  TypeScript 5.5+ infers the predicate
// `(param: string) => param is "js-a" | "js-b"`, which SvelteKit's generated
// `MatcherParam<typeof match>` uses to narrow the route param at consumer
// sites.  This file must produce ZERO TS8010/TS8037 — see
// `src/routes/issue-2939-js-params-matcher/[slug=restrictedjs]/+page.svelte`.
export const match = (param) => param === 'js-a' || param === 'js-b';
