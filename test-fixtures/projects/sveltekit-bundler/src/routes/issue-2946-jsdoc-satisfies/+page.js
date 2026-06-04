// Parity fixture for upstream language-tools #2946 / commit d69eb726.
//
// A *checked* `.js` `load` const that the user already types with a leading
// JSDoc `@satisfies` tag.  The route transform must detect the existing tag
// and skip injecting its function-like `@param` (or a second `@satisfies`),
// mirroring upstream's shared `hasTypeDefinition` gate
// (`!isTsFile && getJSDocTags(...).some(t => t.tagName.text === 'satisfies')`).
//
// Must produce ZERO TS8010 (no TS annotation syntax leaks into the `.js`) and
// no duplicate-injection syntax errors.  The `@satisfies {PageLoad}` constrains
// the value's *shape* without widening, so the inferred `event` parameter is
// still the real `PageLoadEvent`: reading `event.url` (real) type-checks, while
// reading `event.bogus` (not on `PageLoadEvent`) surfaces a TS2339 — proving
// the retained JSDoc resolves to real types rather than `any`.
/** @satisfies {import('./$types').PageLoad} */
export const load = (async (event) => {
  void event.url.pathname;
  return { missing: event.bogus };
});
