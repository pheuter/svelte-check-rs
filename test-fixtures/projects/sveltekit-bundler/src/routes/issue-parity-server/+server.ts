// Parity fixture for HTTP endpoint param-annotation handling.
//
// The user deliberately imports `RequestHandler` from `@sveltejs/kit`, which
// types `params` as `Partial<Record<string, string>>`.  That looser shape
// surfaces `params.X is string | undefined` errors — exactly the safety net
// the user opted into.
//
// svelte-check-rs's route transform used to inject an inner
// `: import('./$types.js').RequestEvent` annotation that silently overrode
// the outer `RequestHandler` typing, masking those errors.  This fixture
// proves the override is gone.
import { json, type RequestHandler } from '@sveltejs/kit';

function strictlyString(value: string): string {
  return value.toUpperCase();
}

export const GET: RequestHandler = async ({ params }) => {
  // `params.id` is `string | undefined` under `@sveltejs/kit`'s
  // RequestHandler.  Passing it to a `string`-only function must error
  // with TS2345.
  return json({ id: strictlyString(params.id) });
};
