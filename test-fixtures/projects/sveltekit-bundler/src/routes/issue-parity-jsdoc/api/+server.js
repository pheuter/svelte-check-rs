// Parity fixture: an untyped HTTP endpoint in a *.js* file.
//
// On `.js` the transform must emit a JSDoc callable type
// (`/** @type {(arg0: RequestEvent) => Response | Promise<Response>} */`)
// rather than the TS `event: RequestEvent` annotation, which would be a
// TS8010 error under `checkJs`.  This file must produce ZERO TS8010.
import { json } from '@sveltejs/kit';

export async function GET(event) {
  return json({ path: event.url.pathname });
}
