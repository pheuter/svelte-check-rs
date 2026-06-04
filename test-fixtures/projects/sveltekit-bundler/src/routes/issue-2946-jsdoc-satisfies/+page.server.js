// Parity fixture for upstream language-tools #2946 / commit d69eb726
// ("detect existing JSDoc @satisfies to prevent duplicate injection").
//
// This is a *checked* `.js` file (`allowJs` + `checkJs`).  The user already
// types `actions` with a leading JSDoc `@satisfies` tag.  swc strips comments,
// so the tag is invisible to the AST-span-based `expr_contains_satisfies`
// guard — without the textual JSDoc `@satisfies` detection the route transform
// would inject a *second* `@satisfies` wrap
// (`export const actions = /** @satisfies {Actions} */ (...)`), producing a
// duplicate-type clash / spurious diagnostics on this file.
//
// With the fix the user's single `@satisfies` is left untouched.  To prove it
// carries a *real* `Actions` type (and didn't silently widen to `any`, which
// would make a no-extra-diagnostics assertion vacuous), the `default` action's
// `event` parameter is contextually typed by the retained `@satisfies` as the
// real `RequestEvent`. Reading the non-existent `event.bogus` therefore surfaces
// a genuine TS2339 (mirroring the +page.js load case) — locking in that the
// retained tag resolves to real Kit types, not `any`.
/** @satisfies {import('./$types').Actions} */
export const actions = {
  default: async (event) => {
    void event.request;
    return { x: event.bogus };
  }
};
