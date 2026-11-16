export const meta = {
  name: 'design-package',
  description: 'Rebuild the Aphrody design package from the repo: Drive Doc/Sheet/Slides, Figma file, hosting inventory, CI state, then a completeness check',
  whenToUse: 'After docs/spec.md, docs/adr/*, docs/features.toml or brand/ change, or when the owner asks to refresh the Drive, Figma or roadmap mirrors.',
  phases: [
    { title: 'Read', detail: 'repo sources and live hosting state' },
    { title: 'Publish', detail: 'Drive Doc and Sheet, Slides, Figma, roadmap calendar' },
    { title: 'Check', detail: 'read every artifact back and list what is missing' },
  ],
}

// args: { repo: 'C:\\src\\ay-next', driveFolderId: '...', docId?: '...', sheetId?: '...', slidesId?: '...', figmaFileKey?: '...', today: 'YYYY-MM-DD' }
const A = args || {}
const repo = A.repo || 'C:\\src\\ay-next'
const sources = `${repo}\\docs\\spec.md, ${repo}\\docs\\architecture.md, ${repo}\\docs\\adr\\*.md, ${repo}\\docs\\features.toml, ${repo}\\ay.toml, ${repo}\\brand`
const rule = 'Read the sources; never invent content. Load MCP tools with ToolSearch before calling them. Report ids and URLs of what you created or updated.'

phase('Read')
const [hosting, ci] = await parallel([
  () => agent(
    `Inventory the live hosting state that the cloud product (docs/adr/0006-cloud.md) replaces or uses.
     Use the MCP servers: plugin:cloudflare (search, then execute: Workers, Pages, R2, DNS zones), plugin:vercel (list_projects, list_domains),
     plugin:aphrody:ovhcloud (get-vps-list, get-domain-list, list-instances), plugin:hostinger (search then execute: websites, domains).
     Read only, change nothing. Return a table: provider, resource kind, name, domain, status.`,
    { label: 'hosting inventory', phase: 'Read' }),
  () => agent(
    `With plugin:github (or gh), read the last 5 workflow runs of ${ (A.owner || 'aphrody-dev') }/ay and of aphrody-labs/bun:
     name, head sha, conclusion, and for failures the first error line. Return them as a short table.`,
    { label: 'CI state', phase: 'Read' }),
])

phase('Publish')
const published = await parallel([
  () => agent(
    `Update the Google Doc ${A.docId ? `id ${A.docId}` : '"ay — Spec, architecture and decisions"'} in Drive folder ${A.driveFolderId}
     so it mirrors ${sources}. If it does not exist, create it with mcp__claude_ai_Google_Drive__create_file
     (contentMimeType text/markdown, parentId the folder). Use mcp__claude_ai_Google_Docs__update_doc to update an existing one. ${rule}`,
    { label: 'Drive doc', phase: 'Publish' }),
  () => agent(
    `Update the Google Sheet ${A.sheetId ? `id ${A.sheetId}` : '"ay — Feature matrix"'} in Drive folder ${A.driveFolderId}:
     tab "Matrix" = every [[feature]] row of ${repo}\\docs\\features.toml (product, id, title, layer, status, proof, proof_kind, reference),
     tab "Inventory" kept as is, and a tab "Hosting" with this inventory:\n${hosting}\n
     Use the Google Sheets MCP (get_spreadsheet, batch_clear_values, update_values, update_spreadsheet). ${rule}`,
    { label: 'Drive sheet', phase: 'Publish' }),
  () => agent(
    `Update the Google Slides deck ${A.slidesId ? `id ${A.slidesId}` : '"ay — Architecture and roadmap"'} in Drive folder ${A.driveFolderId}
     from ${sources}: products, quality bar, tree, toolchain, OS, browser/search/cloud, feature matrix counts, order and blockers.
     Use the Google Slides MCP (read_presentation, update_presentation); create it as a pptx upload through Drive create_file only if missing. ${rule}`,
    { label: 'slides', phase: 'Publish' }),
  () => agent(
    `Update the Figma file ${A.figmaFileKey ? `key ${A.figmaFileKey}` : 'named "Aphrody" in team "ay"'} so its Brand, Design system, Search, OS, Browser and Cloud pages
     match ${repo}\\brand and the ui rows of ${repo}\\docs\\features.toml. Read skill://figma/figma-use/SKILL.md first (MCP resource of the Figma server),
     create_session before file tools, and get_screenshot every frame you changed and check it. ${rule}`,
    { label: 'figma', phase: 'Publish' }),
  () => agent(
    `In Google Calendar (mcp__claude_ai_Google_Calendar__*), keep one all-day event per product milestone of the delivery order in ${repo}\\docs\\spec.md
     (brand, craft, search, cloud, browser, os) titled "ay: <product> replica" with the product's feature rows and done/total counts from
     ${repo}\\docs\\features.toml in the description. Update existing events (search_events "ay:") instead of duplicating; add no dates the
     sources don't give, put undated milestones as an event on ${A.today} marked "to schedule". ${rule}`,
    { label: 'roadmap calendar', phase: 'Publish' }),
])

phase('Check')
const gaps = await agent(
  `Completeness check of the Aphrody design package. Read back every artifact with the read tools
   (Drive read_file_content, Sheets get_values, Slides read_presentation, Figma get_metadata/get_screenshot, Calendar search_events "ay:").
   Compare each to ${sources}. Publish results:\n${published.map((p, i) => `#${i}: ${p}`).join('\n')}\nCI:\n${ci}\n
   Return: what matches, what is stale or missing (artifact, item, why), and the CI failures that block delivery.`,
  { label: 'completeness', phase: 'Check' })

return { hosting, ci, published, gaps }
