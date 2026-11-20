#!/bin/sh
# Installs the prod host LiteLLM startup from the repository (from the src root):
#   sh apps/ai-gateway/systemd/install-prod.sh [ssh arguments, then root@<prod-host>]
# default: hop through the test host over the WireGuard mesh (host key verified under the prod host's public IP).
# Prerequisite credentials in /etc/credstore.encrypted: CLOUDFLARE_API_TOKEN, LITELLM_MASTER_KEY,
# ANTHROPIC_API_KEY, ANTHROPIC_WORKSPACE_ID, VERTEX_APHRODY_SA_KEY (valeur en stdin de
# `systemd-creds encrypt --name=<NOM> - /etc/credstore.encrypted/<NOM>`).
for _s in /usr/local/lib/aphrody/site.sh "$(dirname "$0")/../../../packages/env-kit/loaders/shell/site.sh"; do [ -f "$_s" ] && { . "$_s"; break; }; done
set -eu
[ $# -gt 0 ] || set -- -o BatchMode=yes -o HostKeyAlias=$(site_get hosts.prod.public_ip) -J "$(site_get hosts.test.ssh)" root@$(site_get hosts.prod.mesh_ip)
tar -cf - -C apps/ai-gateway ai.toml litellm-overlay.toml litellm-serve.ts systemd/ay-ai-litellm.service.d/10-creds.conf |
  ssh "$@" 'set -e; d=/usr/local/lib/aphrody-ai; install -d -m 0755 $d /etc/systemd/system/ay-ai-litellm.service.d
  tar -xf - -C $d --no-same-owner
  install -m 0644 $d/systemd/ay-ai-litellm.service.d/10-creds.conf /etc/systemd/system/ay-ai-litellm.service.d/10-creds.conf
  rm -rf $d/systemd; chmod 0644 $d/*
  systemctl daemon-reload && systemctl restart ay-ai-litellm && sleep 25 && systemctl is-active ay-ai-litellm'
