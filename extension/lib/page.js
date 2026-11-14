// A fragment that starts with "/" or "!" is a route on a hash-routed site, so it names a
// different view, not a place in the same one.
const ROUTE_FRAGMENT = /^#[/!]/;

export function withoutFragment(url) {
  const at = url.indexOf("#");
  if (at === -1 || ROUTE_FRAGMENT.test(url.slice(at))) return url;
  return url.slice(0, at);
}

export function highlightUrl(pageUrl, declaredUrl) {
  return withoutFragment(declaredUrl || pageUrl);
}
