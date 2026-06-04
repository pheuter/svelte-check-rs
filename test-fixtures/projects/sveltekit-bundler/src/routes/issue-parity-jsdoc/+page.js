// Parity fixture for SvelteKit "zero-types" JSDoc transform helpers
// (upstream language-tools #2939 / commit b914d010).
//
// This is a *.js* route file, and `tsconfig.json` turns on `allowJs` +
// `checkJs`.  svelte-check-rs's route transform used to inject TypeScript
// type-annotation syntax (`load(event: PageLoadEvent)`, `export const ssr:
// boolean`) unconditionally — which is illegal in a checked `.js` file and
// produces TS8010 ("Type annotation can only be used in TypeScript files").
//
// The fix branches on the file extension and emits JSDoc instead.  This
// fixture must therefore produce ZERO TS8010 diagnostics.
//
// To prove the injected JSDoc carries *real* types (and didn't silently
// resolve to `any`, which would make the no-TS8010 assertion vacuous), the
// load function reads a property that does NOT exist on `PageLoadEvent`,
// which must surface a TS2339 at line 20.  `event.url` is real; `event.bogus`
// is not.
export function load(event) {
  void event.url.pathname;
  return { missing: event.bogus };
}

// Untyped config exports get JSDoc `@type` casts on the `.js` path
// (`/** @type {boolean} */ (true)`), never `export const prerender: boolean`.
export const prerender = true;
export const csr = false;
