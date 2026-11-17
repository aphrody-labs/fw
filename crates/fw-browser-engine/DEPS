# DEPS: what this repository depends on without importing it, in gclient's syntax (Chromium's DEPS format).
# Everything imported with its history is pinned in tools/upstreams.lock.json instead; `bun testing/acceptance.ts track`
# reads both and reports how far each pin is behind its upstream.

vars = {
  'chromium_git': 'https://chromium.googlesource.com',
  # Chrome for Testing canary (Linux) the baselines run today; build/engines builds the engines from this revision.
  'chromium_version': '160.0.8166.0',
}

deps = {
  # The checkout lives on the build host (CHROMIUM_SRC) and in third_party/chromium on a workstation; git ignores it.
  'third_party/chromium/src': Var('chromium_git') + '/chromium/src.git' + '@' + Var('chromium_version'),
}
