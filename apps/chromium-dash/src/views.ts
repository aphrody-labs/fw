import { l10n } from "./html.ts";
import {
  type ChannelStatus,
  type ChannelsPage,
  type Commit,
  type CommitSummary,
  type Feature,
  type FeatureList,
  type FeatureSummary,
  type Gate,
  type Milestone,
  type MilestoneFeatures,
  type Minibranch,
  type Release,
  type ScheduleMilestone,
  type CrosBuilds,
  type ChannelSchedule,
  CHANNELS,
} from "@aphrody/browser-chromium-releases";
import { ago, dateOf, scheduleDay, statusDate } from "./format.ts";
import { html, raw, type Safe } from "./html.ts";

const chip = (m: number | string | null | undefined) =>
  m ? html`<span class="chip m${Number(m) % 4}">${m}</span>` : "";
const shortHash = (h: string) => h.slice(0, 12);

export function tabs(items: { href: string; label: string; current: boolean }[]): Safe {
  return html`<div class="tabs">
    ${items.map((i) => html`<a href="${i.href}" ${i.current ? raw(' aria-current="page"') : ""}>${i.label}</a>`)}
  </div>`;
}

/** One release card: header with the channel and its newest version, then rows. */
export function releaseCard(channel: string, rows: Release[], more?: string): Safe {
  const head = rows[0];
  return html`<section class="card ${channel.toLowerCase()}" data-channel="${channel}">
    <header><span>${channel}</span><span>${head?.version ?? "no release"}</span></header>
    ${rows.map(
      (r) =>
        html`<div class="row">
          ${chip(r.milestone)}<span class="grow mono">${r.version}</span>
          <span class="meta">${r.chromium_main_branch_position ?? ""} @ ${dateOf(r.time)}</span>
          <a href="/commit/${r.hashes.chromium}" title="chromium ${r.hashes.chromium}">${l10n("info")}</a>
        </div>`,
    )}
    ${more ? html`<a class="more" href="${more}">${l10n("load_more")}</a>` : ""}
  </section>`;
}

export function home(p: {
  platform: string;
  channelRows: { channel: string; release?: Release }[];
  milestones: Milestone[];
  schedule: ScheduleMilestone[];
  channels: ChannelStatus[];
  measuredWeeks?: number;
}): Safe {
  return html`<h1>${l10n("chromium_and_chrome_releases")}</h1>
    <p class="lead">
      ${l10n("release_channels_the_milestone_schedule_branches")}
    </p>
    <h2>${l10n("has_it_shipped")}</h2>
    <p class="lead">
      ${l10n("look_a_commit_up_on_the")} <a href="/commits">${l10n("commits_2")}</a> ${l10n("page_to_see_the_first_chrome_version_that_carrie")}
    </p>
    <div class="tabs">
      <span class="muted">Newest on ${p.platform}</span>${p.channelRows.map(
        (r) =>
          html`<span class="pill" data-channel="${r.channel}"
            ><b>${r.channel}</b> ${chip(r.release?.milestone)} ${r.release?.version ?? "none"}</span
          >`,
      )}
    </div>
    <h2>${l10n("schedule_and_branches")}</h2>
    <p class="lead">
      ${l10n("which_branch_is_which_milestone_is_on")} <a href="/branches">${l10n("branches_2")}</a>; the upcoming dates are on
      <a href="/schedule">${l10n("schedule_2")}</a>.
    </p>
    <div class="sched">${p.schedule.slice(0, 3).map((m) => scheduleCard(m, false))}</div>
    <h2>${l10n("milestones")}</h2>
    ${milestoneTable(p.milestones.slice(0, 6))}
    <h2>${l10n("release_channels")}</h2>
    <p class="lead">
      ${p.measuredWeeks ? `Stable has shipped a major version every ${p.measuredWeeks} weeks on the live schedule. ` : ""}<a
        href="/channels"
        >${l10n("channels_cadence_and_how_to_switch")}</a
      >.
    </p>
    <table>
      <thead>
        <tr>
          <th>${l10n("channel")}</th>
          <th>Newest on ${p.platform}</th>
          <th>${l10n("cadence")}</th>
        </tr>
      </thead>
      <tbody>
        ${p.channels.map(
          (c) =>
            html`<tr>
              <td>${c.name}</td>
              <td class="mono">${c.release?.version ?? ""}</td>
              <td>${c.cadence}</td>
            </tr>`,
        )}
      </tbody>
    </table>`;
}

export function milestoneTable(rows: Milestone[]): Safe {
  return html`<table>
    <thead>
      <tr>
        <th>${l10n("milestone")}</th>
        <th>${l10n("chromium")}</th>
        <th>${l10n("skia")}</th>
        <th>V8</th>
        <th>${l10n("webrtc")}</th>
        <th>${l10n("pdfium")}</th>
        <th>${l10n("phase")}</th>
      </tr>
    </thead>
    <tbody>
      ${rows.map(
        (m) =>
          html`<tr>
            <td>${chip(m.milestone)}</td>
            <td>${m.chromium_branch ?? ""}</td>
            <td>${m.skia_branch ?? ""}</td>
            <td>${m.v8_branch ?? ""}</td>
            <td>${m.webrtc_branch ?? ""}</td>
            <td>${m.pdfium_branch ?? ""}</td>
            <td>${m.schedule_phase ?? ""}</td>
          </tr>`,
      )}
    </tbody>
  </table>`;
}

const SCHEDULE_ROWS: [keyof ScheduleMilestone, string][] = [
  ["feature_freeze", "Feature Freeze"],
  ["branch_point", "Branch"],
  ["earliest_beta", "Beta Promotion"],
  ["early_stable_cut", "Early Stable Cut"],
  ["early_stable", "Early Stable Release"],
  ["early_stable_cut_ios", "Early Stable Cut (iOS)"],
  ["early_stable_ios", "Early Stable Release (iOS)"],
  ["stable_cut", "Stable Cut"],
  ["stable_date", "Stable Release"],
  ["stable_refresh_first", "Stable Refresh"],
  ["late_stable_date", "Late Stable"],
];

export function scheduleCard(m: ScheduleMilestone, owners = true, now = Date.now()): Safe {
  const owner = Object.entries(m.ldaps ?? {}).filter(([, v]) => v);
  return html`<section class="card" data-milestone="${m.mstone}">
    <header>${m.mstone}</header>
    ${SCHEDULE_ROWS.filter(([k]) => m[k]).map(([k, label]) => html`<div class="row${Date.parse(`${m[k]}Z`) < now ? " past" : ""}"><span>${label}</span><span>${scheduleDay(m[k] as string)}</span></div>`)}
    ${owners ? owner.map(([k, v]) => html`<div class="row"><span>${k.replaceAll("_", " ")} owner</span><span>${v}</span></div>`) : ""}
  </section>`;
}

export function schedule(rows: ScheduleMilestone[], prev: string, next: string): Safe {
  return html`<div class="pager"><a href="${prev}">&lt; PREV</a><a href="${next}">NEXT &gt;</a></div>
    <div class="sched">${rows.map((m) => scheduleCard(m))}</div>`;
}

export function branches(ms: Milestone[], mini: Minibranch[]): Safe {
  return html`<p class="lead">${l10n("which_branch_of_each_repository_belongs_to_which")}</p>
    <table>
      <thead>
        <tr>
          <th>${l10n("milestone")}</th>
          <th>${l10n("chromium")}</th>
          <th>${l10n("skia")}</th>
          <th>V8</th>
          <th>${l10n("webrtc")}</th>
          <th>${l10n("angle")}</th>
          <th>${l10n("dawn")}</th>
          <th>${l10n("devtools")}</th>
          <th>${l10n("pdfium")}</th>
          <th>${l10n("branch_point")}</th>
        </tr>
      </thead>
      <tbody>
        ${ms.map(
          (m) =>
            html`<tr>
              <td>${chip(m.milestone)}</td>
              <td>${m.chromium_branch ?? ""}</td>
              <td>${m.skia_branch ?? ""}</td>
              <td>${m.v8_branch ?? ""}</td>
              <td>${m.webrtc_branch ?? ""}</td>
              <td>${m.angle_branch ?? ""}</td>
              <td>${m.dawn_branch ?? ""}</td>
              <td>${m.devtools_branch ?? ""}</td>
              <td>${m.pdfium_branch ?? ""}</td>
              <td class="mono">
                ${m.chromium_main_branch_hash ? html`<a href="/commit/${m.chromium_main_branch_hash}">${shortHash(m.chromium_main_branch_hash)}</a>` : ""}
              </td>
            </tr>`,
        )}
      </tbody>
    </table>
    <h2>${l10n("minibranches")}</h2>
    <table>
      <thead>
        <tr>
          <th>${l10n("milestone")}</th>
          <th>${l10n("chromium")}</th>
          <th>${l10n("skia")}</th>
          <th>V8</th>
          <th>${l10n("expires")}</th>
        </tr>
      </thead>
      <tbody>
        ${mini.slice(0, 12).map(
          (m) =>
            html`<tr>
              <td>${chip(m.milestone)}</td>
              <td>${m.chromium_minibranch}</td>
              <td>${m.skia_minibranch}</td>
              <td>${m.v8_minibranch}</td>
              <td>${m.expire_time ?? ""}</td>
            </tr>`,
        )}
      </tbody>
    </table>`;
}

export function commitsPage(q: string, recent: CommitSummary[], found?: Commit, error?: string): Safe {
  return html`<form class="search" action="/commits" method="get">
      <input type="search" name="q" value="${q}" placeholder="${l10n("commit_hash_1_to_40_hex_digits")}" /><button>
        ${l10n("look_up")}
      </button>
    </form>
    ${error ? html`<p class="stale">${error}</p>` : ""} ${found ? commitBody(found) : ""}
    <h2>${l10n("recent_commits")}</h2>
    <table>
      <thead>
        <tr>
          <th>${l10n("commit")}</th>
          <th>${l10n("repo")}</th>
          <th>${l10n("branch")}</th>
          <th>${l10n("title")}</th>
          <th>${l10n("date")}</th>
        </tr>
      </thead>
      <tbody>
        ${recent.map(
          (c) =>
            html`<tr>
              <td class="mono"><a href="/commit/${c.git_hash}">${shortHash(c.git_hash)}</a></td>
              <td>${c.repo}</td>
              <td>${c.branch}</td>
              <td>${c.title}</td>
              <td>${dateOf(c.time)}</td>
            </tr>`,
        )}
      </tbody>
    </table>`;
}

export function commitBody(c: Commit): Safe {
  const deploy = Object.entries(c.deployment ?? {});
  return html`<div class="panel">
    <h2>${c.title}</h2>
    <dl class="kv">
      <dt>${l10n("commit")}</dt>
      <dd class="mono">${c.git_hash}</dd>
      <dt>${l10n("repository")}</dt>
      <dd>${c.repo} (${c.branch}${c.position ? ` @{#${c.position}}` : ""})</dd>
      <dt>${l10n("author")}</dt>
      <dd>${c.author}</dd>
      <dt>${l10n("date")}</dt>
      <dd>${dateOf(c.time)}</dd>
      <dt>${l10n("first_landed_in")}</dt>
      <dd>${c.first_landed ? html`<b>${c.first_landed}</b>` : "not in a release yet"}</dd>
      <dt>${l10n("earliest_release")}</dt>
      <dd>${c.earliest ?? "none"}</dd>
      ${deploy.map(
        ([ch, v]) =>
          html`<dt>${ch}</dt>
            <dd>${v ?? "not shipped"}</dd>`,
      )}
      <dt>${l10n("source")}</dt>
      <dd><a href="${c.source_tree}">${c.source_tree}</a></dd>
      ${
        c.review_info
          ? html`<dt>${l10n("review")}</dt>
              <dd><a href="${c.review_info.code_review_url}">${c.review_info.change_id}</a></dd>`
          : ""
      }
    </dl>
    <h2>${l10n("message")}</h2>
    <pre class="mono" style="white-space:pre-wrap">${c.message}</pre>
  </div>`;
}

export function servingBuilds(b: CrosBuilds): Safe {
  const rows = Object.entries(b.builds).sort(([a], [z]) => a.localeCompare(z));
  return html`<p class="lead">
      ChromeOS boards, their devices and the milestone of their last update (${rows.length} boards).
    </p>
    <table>
      <thead>
        <tr>
          <th>${l10n("board")}</th>
          <th>${l10n("devices")}</th>
          <th>${l10n("first_milestone")}</th>
          <th>${l10n("auto_update_expired")}</th>
        </tr>
      </thead>
      <tbody>
        ${rows.slice(0, 200).map(
          ([board, v]) =>
            html`<tr>
              <td class="mono">${board}</td>
              <td>${(v.brandNames ?? []).join(", ")}</td>
              <td>${v.fsiMilestoneNumber}</td>
              <td>${v.isAue ? "yes" : ""}</td>
            </tr>`,
        )}
      </tbody>
    </table>`;
}

export function channelsPage(p: {
  channels: ChannelStatus[];
  page?: ChannelsPage;
  measuredWeeks?: number;
  platform: string;
  source: { page: string; doc: string };
}): Safe {
  return html`<p class="lead">
      Chrome rolls changes out through channels, from the daily Canary build to Stable.
      ${p.page ? html`The chromium.org page says: ${p.page.intro}` : ""}
    </p>
    ${p.measuredWeeks ? html`<p class="lead">${l10n("measured_on_the_live_schedule_a_new_stable_major")} <b>${p.measuredWeeks} weeks</b>${p.page?.stableMajorWeeks && p.page.stableMajorWeeks !== p.measuredWeeks ? html` (the chromium.org page still says ${p.page.stableMajorWeeks})` : ""}.</p>` : ""}
    <div class="cards">
      ${p.channels.map(
        (c) =>
          html`<section class="card ${c.id}" data-channel="${c.id}">
            <header><span>${c.name}</span><span class="mono">${c.release?.version ?? ""}</span></header>
            <div class="row"><span class="grow">${c.summary}</span></div>
            <div class="row"><span>${l10n("cadence")}</span><span class="meta">${c.cadence}</span></div>
            <div class="row"><span>${l10n("lag")}</span><span class="meta">${c.lag}</span></div>
            <div class="row">
              <span>${l10n("side_by_side")}</span
              ><span class="meta"
                >${c.parallelInstall ? "own profile, runs next to other channels" : "managed by enterprise policy"}</span
              >
            </div>
            <div class="row">
              <span>${l10n("where")}</span
              ><span class="meta"
                >${(["desktop", "android", "ios"] as const).filter((s) => c.availability[s]).join(", ")}</span
              >
            </div>
            ${p.page ? (["desktop", "android", "ios"] as const).filter((s) => p.page!.downloads[s][c.id]).map((s) => html`<div class="row"><span>Get it on ${s}</span><a href="${p.page!.downloads[s][c.id]!}">${l10n("install")}</a></div>`) : ""}
          </section>`,
      )}
    </div>
    <h2>${l10n("how_to_switch_channels")}</h2>
    <ol>
      <li>Back up your profile (the <code>User Data\\Default</code> directory) before you change.</li>
      <li>
        Stable, Beta and Dev share one install location and profile on desktop; Canary has its own and runs next to
        them. Newer builds of the channels run side by side in current Chrome.
      </li>
      <li>
        Moving to a more stable channel puts you on an earlier version: you get no updates until that channel passes the
        version you run. A profile written by a newer version may not open on an older one; keep the backup.
      </li>
      <li>${l10n("turn_on_anonymous_usage_statistics_so_your_chann")}</li>
      <li>
        ${l10n("report_dev_and_canary_problems_at")} <a href="https://crbug.com/new">${l10n("crbug_com_new")}</a> ${l10n("after_reading_the_bug_reporting_guidelines")}
      </li>
    </ol>
    ${
      p.page?.notes.length
        ? html`<h2>${l10n("notes")}</h2>
            <ul>
              ${p.page.notes.map((n) => html`<li>${n}</li>`)}
            </ul>`
        : ""
    }
    <p class="muted">
      ${l10n("sources")} <a href="${p.source.page}">${l10n("chromium_org_page")}</a>, <a href="${p.source.doc}">${l10n("the_original_markdown")}</a>,
      releases from Chromium Dash for ${p.platform}.
    </p>`;
}

// ---- Chrome Status

const featureLink = (f: { id: number; name: string }) => html`<a href="/feature/${f.id}">${f.name}</a>`;

export function featureRow(f: FeatureSummary): Safe {
  return html`<div class="feat">
    <div>
      <div class="name">${featureLink(f)}</div>
      <div class="summary">${(f.summary ?? "").slice(0, 220)}</div>
      <div>
        ${(f.blink_components ?? []).slice(0, 2).map((c) => html`<span class="pill">${c}</span> `)}${f.browsers?.chrome?.status?.text ? html`<span class="pill">${f.browsers.chrome.status.text}</span>` : ""}
      </div>
    </div>
  </div>`;
}

export function roadmap(
  cols: { tag: string; sched: ChannelSchedule; features: MilestoneFeatures }[],
  now = Date.now(),
): Safe {
  return html`<div class="road">
    ${cols.map(({ tag, sched, features }) => {
      const when = Date.parse(`${sched.early_stable ?? sched.stable_date}Z`) || undefined;
      const types = Object.entries(features.features_by_type).filter(([, rows]) => rows.length);
      return html`<section class="card" data-milestone="${sched.mstone}">
        <div class="head">
          <div class="tag">${tag}</div>
          <div class="big">Chrome ${sched.mstone}</div>
          <div><b>${l10n("beta")}</b> ${scheduleDay(sched.earliest_beta)} - ${scheduleDay(sched.latest_beta)}</div>
          <div><b>${l10n("stable")}</b> ${when ? `${ago(when, now)} (${dateOf(when)})` : ""}</div>
        </div>
        <div class="head" style="padding-top:0">
          <span class="muted">${features.total_count} features in this milestone</span>
        </div>
        ${types.map(
          ([type, rows]) =>
            html`<h3>${type}</h3>
              <ul>
                ${rows.map((f) => html`<li>${featureLink(f)}</li>`)}
              </ul>`,
        )}
      </section>`;
    })}
  </div>`;
}

export function featuresPage(p: { q: string; sort: string; start: number; num: number; list: FeatureList }): Safe {
  const href = (start: number) =>
    `/features?${new URLSearchParams({ ...(p.q ? { q: p.q } : {}), ...(p.sort ? { sort: p.sort } : {}), start: String(start), num: String(p.num) })}`;
  const end = Math.min(p.start + p.num, p.list.total_count);
  return html`<form class="search" action="/features" method="get">
      <input
        type="search"
        name="q"
        value="${p.q}"
        placeholder="${l10n("search_features_e_g_webrtc_or_browser_chrome")}"
      /><button>${l10n("search")}</button>
    </form>
    <p class="muted">${p.list.total_count} features${p.list.total_count ? `, showing ${p.start + 1} to ${end}` : ""}</p>
    ${p.list.features.map(featureRow)}
    <div class="pager">
      ${p.start > 0 ? html`<a href="${href(Math.max(0, p.start - p.num))}">&lt; Previous</a>` : ""}${end < p.list.total_count ? html`<a href="${href(p.start + p.num)}">Next &gt;</a>` : ""}
    </div>`;
}

const view = (v: { view: { text: string | null; url: string | null; notes: string | null } }) =>
  v.view.text ?? "No signals";
const links = (urls: string[]) => urls.map((u) => html`<div><a href="${u}">${u}</a></div>`);
const MS = ["desktop", "android", "ios", "webview"] as const;

export function featurePage(f: Feature, gates: Gate[]): Safe {
  const chrome = f.browsers.chrome;
  return html`<p><a href="/features">&larr; All features</a></p>
    <h1>${f.name}</h1>
    <div class="panel">
      <h2>${l10n("overview")}</h2>
      <p style="white-space:pre-wrap">${f.summary}</p>
      <dl class="kv">
        <dt>${l10n("category")}</dt>
        <dd>${f.category}</dd>
        <dt>${l10n("feature_type")}</dt>
        <dd>${f.feature_type}</dd>
        <dt>${l10n("intent_stage")}</dt>
        <dd>${f.intent_stage}</dd>
        <dt>${l10n("blink_components")}</dt>
        <dd>${f.blink_components.map((c) => html`<span class="pill">${c}</span> `)}</dd>
        <dt>${l10n("owners")}</dt>
        <dd>${f.owner_emails.join(", ")}</dd>
        <dt>${l10n("created")}</dt>
        <dd>${statusDate(f.created.when)} by ${f.created.by}</dd>
        <dt>${l10n("last_updated")}</dt>
        <dd>${statusDate(f.updated.when)}</dd>
      </dl>
    </div>
    ${
      f.motivation
        ? html`<div class="panel">
            <h2>${l10n("motivation")}</h2>
            <p style="white-space:pre-wrap">${f.motivation}</p>
          </div>`
        : ""
    }
    <div class="panel">
      <h2>${l10n("specification")}</h2>
      <dl class="kv">
        <dt>${l10n("spec")}</dt>
        <dd>${f.standards.spec ? html`<a href="${f.standards.spec}">${f.standards.spec}</a>` : "none"}</dd>
        <dt>${l10n("spec_status")}</dt>
        <dd>${f.standards.maturity.text ?? ""}</dd>
        <dt>${l10n("explainers")}</dt>
        <dd>${links(f.explainer_links)}</dd>
        <dt>${l10n("web_platform_tests")}</dt>
        <dd>
          ${f.wpt === null ? "" : f.wpt ? "yes" : "no"}${f.wpt_descr ? html` <a href="${f.wpt_descr}">${f.wpt_descr}</a>` : ""}
        </dd>
      </dl>
    </div>
    <div class="panel">
      <h2>${l10n("status_in_chromium")}</h2>
      <dl class="kv">
        <dt>${l10n("status")}</dt>
        <dd>
          ${chrome.status.text}
          ${chrome.status.milestone_str && chrome.status.milestone_str !== chrome.status.text ? `(${chrome.status.milestone_str})` : ""}
        </dd>
        <dt>${l10n("tracking_bug")}</dt>
        <dd>${chrome.bug ? html`<a href="${chrome.bug}">${chrome.bug}</a>` : ""}</dd>
        ${MS.map((k) =>
          chrome[k]
            ? html`<dt>Shipping on ${k}</dt>
                <dd>${chrome[k]}</dd>`
            : "",
        )}
      </dl>
    </div>
    <div class="panel">
      <h2>${l10n("consensus_and_standardization")}</h2>
      <dl class="kv">
        <dt>${l10n("firefox")}</dt>
        <dd>${view(f.browsers.ff)}</dd>
        <dt>${l10n("safari")}</dt>
        <dd>${view(f.browsers.safari)}</dd>
        <dt>${l10n("web_developers")}</dt>
        <dd>${view(f.browsers.webdev)}</dd>
      </dl>
    </div>
    <div class="panel">
      <h2>${l10n("stages")}</h2>
      <table>
        <thead>
          <tr>
            <th>${l10n("stage")}</th>
            <th>${l10n("type")}</th>
            <th>${l10n("desktop")}</th>
            <th>${l10n("android")}</th>
            <th>${l10n("ios")}</th>
            <th>${l10n("webview")}</th>
            <th>${l10n("intent")}</th>
          </tr>
        </thead>
        <tbody>
          ${f.stages.map(
            (s) =>
              html`<tr>
                <td>${s.display_name ?? s.id}</td>
                <td>${s.stage_type}</td>
                <td>${s.desktop_first ?? ""}</td>
                <td>${s.android_first ?? ""}</td>
                <td>${s.ios_first ?? ""}</td>
                <td>${s.webview_first ?? ""}</td>
                <td>${s.intent_thread_url ? html`<a href="${s.intent_thread_url}">${l10n("thread")}</a>` : ""}</td>
              </tr>`,
          )}
        </tbody>
      </table>
    </div>
    <div class="panel">
      <h2>${l10n("review_gates")}</h2>
      <table>
        <thead>
          <tr>
            <th>${l10n("team")}</th>
            <th>${l10n("gate")}</th>
            <th>${l10n("state")}</th>
          </tr>
        </thead>
        <tbody>
          ${gates.map(
            (g) =>
              html`<tr>
                <td>${g.team_name}</td>
                <td>${g.gate_type}</td>
                <td>${g.state}</td>
              </tr>`,
          )}
        </tbody>
      </table>
    </div>`;
}

export function apiPage(
  origin: string,
  dash: readonly { path: string; method: string; params: string; auth: boolean }[],
  status: readonly { path: string; params: string; auth: boolean }[],
): Safe {
  return html`<p class="lead">
      ${l10n("both_services_public_json_served_from_the_cache_")}
    </p>
    <h2>${l10n("chromium_dash")}</h2>
    <table>
      <thead>
        <tr>
          <th>${l10n("path")}</th>
          <th>${l10n("method")}</th>
          <th>${l10n("parameters")}</th>
          <th>${l10n("served")}</th>
        </tr>
      </thead>
      <tbody>
        ${dash.map(
          (e) =>
            html`<tr>
              <td class="mono">${e.auth ? e.path : html`<a href="${e.path}">${e.path}</a>`}</td>
              <td>${e.method}</td>
              <td>${e.params}</td>
              <td>${e.auth ? "needs sign-in" : "yes"}</td>
            </tr>`,
        )}
      </tbody>
    </table>
    <h2>Chrome Status (/api/v0)</h2>
    <table>
      <thead>
        <tr>
          <th>${l10n("path")}</th>
          <th>${l10n("parameters")}</th>
          <th>${l10n("served")}</th>
        </tr>
      </thead>
      <tbody>
        ${status.map(
          (e) =>
            html`<tr>
              <td class="mono">${e.path}</td>
              <td>${e.params}</td>
              <td>${e.auth ? "needs sign-in" : "yes"}</td>
            </tr>`,
        )}
      </tbody>
    </table>
    <h2>${l10n("added_here")}</h2>
    <table>
      <tbody>
        <tr>
          <td class="mono"><a href="/api/channels">${l10n("api_channels")}</a></td>
          <td>The channel dataset joined with live releases (platform)</td>
        </tr>
        <tr>
          <td class="mono"><a href="/api/pick?channel=stable">${l10n("api_pick")}</a></td>
          <td>The Chromium and CEF build to embed (channel, platform)</td>
        </tr>
        <tr>
          <td class="mono"><a href="/healthz">${l10n("healthz")}</a></td>
          <td>${l10n("liveness")}</td>
        </tr>
      </tbody>
    </table>
    <p class="muted">Origin ${origin}. ${CHANNELS.length} channels in the dataset.</p>`;
}
