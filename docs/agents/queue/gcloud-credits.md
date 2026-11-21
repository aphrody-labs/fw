# gcloud: features, identities, credits (queued)

## Goal
Complete inventory of every gcloud identity and project, fix identity setup (named configurations and ADC), enable the useful APIs, read the credits, set a hard billing cap, then turn credits into work (Cloud Build pools, spot L4 VMs, Vertex, Cloud SQL pgvector prepared but not migrated). Commit `configs/claude/scripts/gcloud-inventory.ts`.

## State: inventory only, nothing changed
No resource, budget, API, IAM binding, gcloud configuration or ADC was created or modified. No paid resource runs from this task. No script was written or committed; no SHAs.

## Findings (all read this session, gcloud 594.0.0)
- Identities (`gcloud auth list`): 4. Owner gmail (role `owner`), service account of deleted project rgfr-8927d (dead: PERMISSION_DENIED, project deleted), service account `yolo-automation@aphrody` (sees only `aphrody`), Gemini gmail (ACTIVE; sees only `gen-lang-client-0285144431`, no billing).
- Existing configurations: ai-agent, aphrody-bun-google-automation, default (active, Gemini identity, project rgfr-8927d, dead), org-contact, owner, rosegriffon, sa-automation. `default` points at the deleted project and the Gemini identity: that is why tools fail on `aphrody`.
- Owner sees 12 projects: aphrody, rpbey-483614, rpb-482d5, propane-tranquility-hmhkk, inagle, studio-2936678913-269c4, studio-7582782947-8a5dd, gen-lang-client-0567398758, fc-ayoub, ironic-objectivist-337921, andronow-212103, andronow-294d0.
- Billing accounts seen by owner (7): only `017AB5-0FC923-9CCFE5` "Aphrody" is OPEN; the others are closed (013952-33850C-B1C9B9, 0185BA-0BFDF0-8DDE2E, 01C99E-70AD11-2A7C91, 01E22C-C0EBF3-ACF976, 01EBC5-C9479E-FF1490, 01FE8F-7FAEE1-A9CD9F).
- Billing links: `aphrody` and `inagle` -> 017AB5-0FC923-9CCFE5; `propane-tranquility-hmhkk` -> 019873-C7F8B9-B9F0C6 (not visible to the owner identity); the 9 other projects have billing disabled.
- Enabled services: aphrody 524 (compute, cloudbuild, artifactregistry, batch, run, sqladmin, storage, aiplatform, generativelanguage, cloudscheduler, pubsub, secretmanager, logging, monitoring, bigquery, billingbudgets, iam, serviceusage all already on), inagle 58, propane-tranquility-hmhkk 37. "Activate all features" needs no API enabling on `aphrody`.
- `aphrody` IAM: owner held by the owner gmail and 2 service accounts.
- `aphrody` resources: 0 compute instances, 0 disks, 0 Cloud SQL; buckets aphrody-bunos-repos, aphrody-yolo-archive, aphrody.firebasestorage.app, aphrody_cloudbuild, gcf-v2-sources-468000409790-us-central1, gcf-v2-uploads-468000409790.us-central1.cloudfunctions.appspot.com, langextract-aphrody-us-central1-batch; Artifact Registry docker repos rpbey (about 13.3 GB), bunos (0), gcf-artifacts (72 MB); Cloud Run/function `billing-cap` (us-central1, nodejs24, entry `capBilling`, trigger Pub/Sub `billing-alerts`).
- BigQuery: dataset `billing_export` exists in `aphrody` (not yet queried; table names and credit rows unchecked). `inagle` has no dataset.
- `inagle` and `propane-tranquility-hmhkk` resource listings: not completed (the `compute instances list` call timed out at 120 s in `inagle`).
- A cap already exists, do not rebuild it: topic `projects/aphrody/topics/billing-alerts` (subscriptions `billing-alerts-agent` and the Eventarc one), function `billing-cap` with `DRY_RUN=0` and `CAP_PROJECTS=inagle` only (so `aphrody` is alert-only), source `gs://gcf-v2-sources-468000409790-us-central1/billing-cap/function-source.zip`. The function acts only on budgets named `project-<id>-<N>eur-month` when cost exceeds the amount.
- Budgets on 017AB5-0FC923-9CCFE5 (9): cap-aphrody-aiplatform-30eur, cap-aphrody-functions-5eur, cap-aphrody-run-5eur, cap-aphrody-gemini-api-10eur, cap-inagle-aiplatform-5eur, cap-inagle-gemini-api-5eur (all EUR, per-service, credits excluded, no Pub/Sub), project-aphrody-40eur-month, account-aphrody-50eur-month, project-inagle-5eur-month (credits included, topic `billing-alerts`).

## Credits found
None. The Credits page could not be read: `list_connected_browsers` returned `[]` (Claude in Chrome extension not connected) on repeated tries. gcloud does not expose credit balances. Amount, expiry and eligible SKUs are unknown; do not infer them.

## Remaining steps
1. Read credits: reconnect Claude in Chrome, open `https://console.cloud.google.com/billing/017AB5-0FC923-9CCFE5/credits` (read only). In parallel query `billing_export` in `aphrody` (`bq --project_id=aphrody ls billing_export`, then sum `credits` of the export table).
2. Identity setup: `gcloud config configurations create/activate` one configuration per project and identity (`aphrody` with owner or `yolo-automation@aphrody`; keep Gemini as its own configuration); repoint `default` away from the deleted project; `gcloud auth application-default login` for the owner (or SA key-less impersonation) and set `quota_project` to `aphrody`. Do not delete identities or projects.
3. Quotas: `gcloud compute regions describe <region> --format="yaml(quotas)"` for L4/A100 spot, CPUs; `gcloud alpha services quota` for Cloud Build concurrency; request GPU quota where 0.
4. Cap: arm `aphrody` by adding it to `CAP_PROJECTS` on `billing-cap` (`gcloud functions deploy`/`update` env vars), create a budget named `project-aphrody-<credit-eur>eur-month` with 50/80/95/100% rules and topic `billing-alerts` once the credit amount is known, then publish a dry-run message (`DRY_RUN=1`) to `billing-alerts` and read the function log as evidence.
5. Runner scripts (typed Bun, `configs/claude/scripts/`): `cloudbuild` runner (pool e2-highcpu-32+), spot L4 VM create-run-delete script, `gcloud-inventory.ts` (identities x projects x billing x roles x resources), Cloud SQL Postgres+pgvector preparation (no data migration), cost table against credits and expiry; first real Cloud Build run with time and cost.

## Resume
```
export CLOUDSDK_CORE_ACCOUNT=<owner gmail from gcloud auth list>
gcloud projects list
gcloud billing budgets list --billing-account=017AB5-0FC923-9CCFE5
gcloud functions describe billing-cap --region=us-central1 --project=aphrody
bq --project_id=aphrody ls billing_export
```
