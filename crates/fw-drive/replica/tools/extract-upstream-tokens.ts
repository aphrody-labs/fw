#!/usr/bin/env bun
// Extract design tokens and measured UI facts from the three file-manager upstreams.
// Usage: bun extract-upstream-tokens.ts --dui <DocumentsUI> --cr <chromium-file-manager> --sd <spacedrive> --mc <tokens.xml> --pins <pins.json> --out <json>
// Every value carries "src": "<repo>:<path>:<line>" read from the checked-out source. A fact that no longer
// matches aborts the run, so a stale pin cannot leave stale numbers behind.
import { readFileSync, writeFileSync, readdirSync, existsSync } from "node:fs";
import { join } from "node:path";

const arg = (k: string) => {
  const i = process.argv.indexOf(`--${k}`);
  if (i < 0 || !process.argv[i + 1]) throw new Error(`missing --${k}`);
  return process.argv[i + 1];
};
const R = { dui: arg("dui"), cr: arg("cr"), sd: arg("sd"), mc: arg("mc") };
const OUT = arg("out");
const PINS = JSON.parse(read2(arg("pins")));
function read2(p: string) {
  return readFileSync(p, "utf8");
}

const read = (p: string) => readFileSync(p, "utf8");
const lineAt = (s: string, i: number) => s.slice(0, i).split("\n").length;
const fail = (m: string): never => {
  throw new Error(m);
};
const norm = (p: string) => p.replaceAll("\\", "/");

function reFact(root: string, repo: string, file: string, re: RegExp) {
  const s = read(join(root, file));
  const m = re.exec(s) ?? fail(`re ${repo}:${file} ${re}`);
  const cap = m[1] ?? m[0];
  const idx = m.index + Math.max(0, m[0].indexOf(cap));
  return { value: cap.replace(/\s+/g, " ").trim(), src: `${repo}:${norm(file)}:${lineAt(s, idx)}` };
}

// ---------- Android DocumentsUI ----------
const M3 = "res/flag(com.android.documentsui.flags.use_material3)";
const duiSrc = (f: string, line: number) => `android-documentsui:${norm(f)}:${line}`;
const DUI = (f: string, re: RegExp) => reFact(R.dui, "android-documentsui", f, re);

type Res = { kind: string; name: string; value: string; qual: string; file: string; line: number };
function parseValues(dir: string): Res[] {
  const out: Res[] = [];
  for (const q of readdirSync(join(R.dui, dir), { withFileTypes: true })) {
    if (!q.isDirectory() || !q.name.startsWith("values")) continue;
    for (const f of readdirSync(join(R.dui, dir, q.name))) {
      if (!f.endsWith(".xml")) continue;
      const rel = `${dir}/${q.name}/${f}`;
      const s = read(join(R.dui, rel));
      for (const m of s.matchAll(/<(dimen|color|integer|bool|fraction)\s+name="([^"]+)"[^>]*>\s*([^<]*?)\s*<\/\1>/gs)) {
        out.push({ kind: m[1], name: m[2], value: m[3].replace(/\s+/g, " "), qual: q.name, file: rel, line: lineAt(s, m.index!) });
      }
    }
  }
  return out;
}
const allRes = [...parseValues(M3), ...parseValues("res")];
const byName = new Map<string, Res[]>();
for (const r of allRes) {
  const k = `${r.kind}:${r.name}`;
  if (!byName.has(k)) byName.set(k, []);
  byName.get(k)!.push(r);
}

function resolveRef(v: string, qual: string, depth = 0): string {
  const m = v.match(/^@(dimen|color)\/(.+)$/);
  if (!m || depth > 6) return v;
  const cands = byName.get(`${m[1]}:${m[2]}`);
  if (!cands) return v;
  const inM3 = cands.filter((c) => c.file.startsWith(M3));
  const pick = inM3.find((c) => c.qual === qual) ?? inM3.find((c) => c.qual === "values") ?? cands[0];
  return resolveRef(pick.value, qual, depth + 1);
}
function tokenOf(kind: string, name: string) {
  const all = byName.get(`${kind}:${name}`) ?? fail(`no ${kind} ${name}`);
  const m3 = all.filter((r) => r.file.startsWith(M3));
  const list = m3.length ? m3 : all;
  const base = list.find((r) => r.qual === "values") ?? list[0];
  const overrides: Record<string, unknown> = {};
  for (const r of list) {
    if (r === base) continue;
    overrides[r.qual.replace(/^values-?/, "")] = { value: r.value, resolved: resolveRef(r.value, r.qual), src: duiSrc(r.file, r.line) };
  }
  return { value: base.value, resolved: resolveRef(base.value, base.qual), src: duiSrc(base.file, base.line), ...(Object.keys(overrides).length ? { overrides } : {}) };
}
const T = (n: string) => tokenOf(["dimen", "integer", "fraction"].find((k) => byName.has(`${k}:${n}`)) ?? "dimen", n);
const C = (n: string) => tokenOf("color", n);
const group = (names: string[], f: (n: string) => unknown = T) => Object.fromEntries(names.map((n) => [n, f(n)]));

function xmlEl(file: string, id: string, attr: string) {
  const s = read(join(R.dui, file));
  for (const m of s.matchAll(/<([A-Za-z][\w.]*)((?:\s+[^<>]*?)?)\s*\/?>/gs)) {
    const body = m[2];
    const idm = body.match(/android:id\s*=\s*"([^"]+)"/);
    if (!idm || idm[1] !== id) continue;
    const am = body.match(new RegExp(`(?:android|app):${attr}\\s*=\\s*"([^"]+)"`));
    if (!am) continue;
    return { value: am[1], src: duiSrc(file, lineAt(s, m.index! + m[1].length + 1 + body.indexOf(am[0]))) };
  }
  return fail(`xml ${file} ${id} ${attr}`);
}
const str = (n: string) => DUI("res/values/strings.xml", new RegExp(`<string name="${n}"[^>]*>([^<]*)`));
const strM3 = (n: string) => DUI(`${M3}/values/strings_m3.xml`, new RegExp(`<string name="${n}"[^>]*>([^<]*)`));
const layoutSel = (qual: string) => DUI(`${M3}/${qual}/layouts_m3.xml`, /name="files_activity_m3" type="layout">([^<]+)</);

function spacing(): Record<string, unknown> {
  const rel = `${M3}/values/spaces.xml`;
  const s = read(join(R.dui, rel));
  return Object.fromEntries([...s.matchAll(/<dimen name="([^"]+)">([^<]+)<\/dimen>/g)].map((m) => [m[1], { value: m[2], src: duiSrc(rel, lineAt(s, m.index!)) }]));
}

const KEYS = "src/com/android/documentsui";
const keyRow = (keys: string, action: string, file: string, re: RegExp) => ({ keys, action, ...DUI(`${KEYS}/${file}`, re) });

const android = {
  windowClasses: {
    compact_lt600dp: { shell: layoutSel("values"), rootsDrawerWidth: xmlEl(`${M3}/layout/drawer_layout_m3.xml`, "@+id/drawer_roots", "layout_width"), edgeSwipe: T("drawer_edge_width_m3") },
    medium_w600dp_h600dp: { shell: layoutSel("values-w600dp-h600dp"), railContainerWidth: xmlEl(`${M3}/layout/nav_rail_layout.xml`, "@+id/nav_rail_container", "layout_width"), snackbarMax: T("snackbar_max_width"), dialogWidthPercent: T("dialog_width_percentage") },
    expanded_w900dp_h600dp: { shell: layoutSel("values-w900dp-h600dp"), rootsWidth: xmlEl(`${M3}/layout/fixed_layout_m3.xml`, "@+id/container_roots", "layout_width"), dockedSearch: DUI(`${M3}/values-w900dp-h600dp/config_m3.xml`, /name="show_docked_search_m3">([^<]+)</) },
  },
  container: group(["layout_padding_top", "layout_padding_end", "layout_padding_bottom", "main_container_padding_start", "main_container_padding_end", "main_container_padding_top", "main_container_section_gap", "main_container_corner_radius_large", "main_container_corner_radius_small"]),
  containerSurface: {
    sectionFill: DUI(`${M3}/drawable/main_container_top_section_background.xml`, /<solid android:color="([^"]+)"/),
    shellFill: DUI(`${M3}/layout/nav_rail_layout.xml`, /background="(\?attr\/colorSurfaceContainer)"/),
    appBackground: C("app_background_color_m3"),
  },
  roots: {
    ...group(["drawer_item_height", "drawer_padding_horizontal", "drawer_padding_top", "drawer_padding_bottom", "drawer_item_text_margin_start", "drawer_item_action_icon_margin_start", "drawer_divider_padding_horizontal", "root_icon_size_m3", "icon_size_m3", "root_action_icon_size_m3", "nav_rail_item_height", "nav_rail_item_icon_bg_width", "nav_rail_item_icon_bg_height", "nav_rail_item_icon_bg_radius", "nav_rail_burger_icon_size", "root_info_header_height_m3", "root_info_header_horizontal_padding_m3", "max_drawer_width_m3"]),
    drawerItemPaddingStart: DUI(`${M3}/values/styles_m3.xml`, /name="DrawerItemStyle"[\s\S]*?paddingStart">([^<]+)</),
    drawerItemRadius: DUI(`${M3}/drawable/root_item_background_m3.xml`, /<corners android:radius="([^"]+)"/),
    activeContainer: DUI(`${M3}/drawable/root_item_background_m3.xml`, /android:state_activated="true"[\s\S]*?android:color="([^"]+)"/),
    textActive: DUI(`${M3}/color/item_root_primary_text_m3.xml`, /state_activated="true" android:color="([^"]+)"/),
    textIdle: DUI(`${M3}/color/item_root_primary_text_m3.xml`, /<item android:color="([^"]+)" \/>/),
    railLabelActive: DUI(`${M3}/color/nav_rail_item_text_color.xml`, /state_activated="true" android:color="([^"]+)"/),
    railPaddingHorizontal: DUI(`${M3}/values/styles_m3.xml`, /name="NavRailStyle"[\s\S]*?paddingHorizontal">([^<]+)</),
    primaryTextStyle: DUI(`${M3}/values/styles_text_m3.xml`, /name="DrawerMenuPrimaryM3" parent="@style\/TextAppearance\.DocumentsUI\.([A-Za-z.]+)"/),
    secondaryTextStyle: DUI(`${M3}/values/styles_text_m3.xml`, /name="DrawerMenuSecondaryM3" parent="@style\/TextAppearance\.DocumentsUI\.([A-Za-z.]+)"/),
    headerTextStyle: DUI(`${M3}/values/styles_text_m3.xml`, /name="DrawerMenuHeaderM3" parent="@style\/TextAppearance\.DocumentsUI\.([A-Za-z.]+)"/),
  },
  searchAndToolbar: {
    ...group(["docked_searchbar_height", "docked_searchbar_margin", "docked_searchbar_padding", "docked_searchbar_icon_padding", "docked_searchbar_text_width", "search_bar_radius_m3", "search_bar_text_size_m3", "search_bar_background_margin_start_m3", "toolbar_padding_start", "toolbar_padding_end", "button_touch_size_m3", "icon_size_headline_large", "action_mode_text_size_m3"]),
    dockedFill: DUI(`${M3}/drawable/docked_search_bar_background.xml`, /<solid android:color="([^"]+)"/),
    dockedRadius: DUI(`${M3}/drawable/docked_search_bar_background.xml`, /<corners android:radius="([^"]+)"/),
    compactBarFill: DUI(`${M3}/drawable/search_bar_background_m3.xml`, /<solid android:color="(\?android:attr\/colorBackgroundFloating)"/),
    toolbarHeight: xmlEl(`${M3}/layout/directory_app_bar_m3.xml`, "@+id/toolbar", "layout_height"),
    toolbarFill: xmlEl(`${M3}/layout/directory_app_bar_m3.xml`, "@+id/toolbar", "background"),
    toolbarTitleStyle: DUI(`${M3}/values/styles_text_m3.xml`, /name="AppBarTitle" parent="@style\/TextAppearance\.DocumentsUI\.([A-Za-z.]+)"/),
    readOnlyLabelRadius: DUI(`${M3}/drawable/read_only_label_background.xml`, /<corners android:radius="([^"]+)"/),
    readOnlyLabelFill: DUI(`${M3}/drawable/read_only_label_background.xml`, /<solid android:color="([^"]+)"/),
    selectionBarFill: xmlEl(`${M3}/layout/directory_app_bar_m3.xml`, "@+id/selection_bar", "background"),
  },
  breadcrumb: group(["breadcrumb_height", "breadcrumb_item_height_m3", "breadcrumb_item_padding_horizontal", "breadcrumb_item_padding_vertical", "breadcrumb_item_max_width", "breadcrumb_item_arrow_size", "breadcrumb_padding_horizontal"]),
  list: {
    ...group(["list_item_height_m3", "list_item_padding_start", "list_item_padding_end", "list_item_padding_vertical", "list_item_inner_padding", "list_item_icon_wrapper_size", "list_item_icon_size", "list_item_icon_wrapper_margin_end", "list_item_thumbnail_corner_radius", "check_icon_size_m3", "list_item_preview_icon_size", "list_container_padding_m3", "list_container_padding_bottom", "doc_header_height_m3", "list_header_cell_radius", "list_header_inner_padding", "list_header_outer_margin", "briefcase_icon_size_m3", "fastscroll_default_thickness_m3", "focus_ring_width", "focus_ring_gap", "hover_overlay_alpha", "ripple_overlay_alpha"]),
    selectedFill: DUI(`${M3}/drawable/list_item_background_m3.xml`, /<item android:state_selected="true">\s*<shape>\s*<corners[^>]*\/>\s*<solid android:color="([^"]+)"/),
    selectedShapeRadius: DUI(`${M3}/drawable/list_item_background_m3.xml`, /<corners android:radius="([^"]+)"/),
    itemMinHeight: xmlEl(`${M3}/layout/item_doc_list_m3.xml`, "@+id/item_root", "minHeight"),
    titleStyle: DUI(`${M3}/values/styles_text_m3.xml`, /name="FileItemLabelText" parent="@style\/TextAppearance\.DocumentsUI\.([A-Za-z.]+)"/),
    captionStyle: DUI(`${M3}/values/styles_text_m3.xml`, /name="ItemCaptionTextM3" parent="@style\/TextAppearance\.DocumentsUI\.([A-Za-z.]+)"/),
    headerStyle: DUI(`${M3}/values/styles_text_m3.xml`, /name="ListTableHeaderText" parent="@style\/TextAppearance\.DocumentsUI\.([A-Za-z.]+)"/),
    wideColumnWeights: {
      name: xmlEl(`${M3}/layout-w900dp-h600dp/item_doc_list_m3.xml`, "@+id/title_container", "layout_weight"),
      type: xmlEl(`${M3}/layout-w900dp-h600dp/item_doc_list_m3.xml`, "@+id/file_type", "layout_weight"),
      size: xmlEl(`${M3}/layout-w900dp-h600dp/item_doc_list_m3.xml`, "@+id/size", "layout_weight"),
      date: xmlEl(`${M3}/layout-w900dp-h600dp/item_doc_list_m3.xml`, "@+id/date", "layout_weight"),
    },
    thumbCardFill: DUI(`${M3}/values/styles_m3.xml`, /name="ThumbnailCardViewStyle"[\s\S]*?cardBackgroundColor">([^<]+)</),
  },
  grid: {
    ...group(["grid_width_m3", "grid_height", "grid_item_margin_m3", "grid_item_thumbnail_width", "grid_item_thumbnail_height", "grid_item_thumbnail_radius", "grid_item_icon_width", "grid_item_icon_height", "grid_item_nameplate_width", "grid_item_nameplate_height", "grid_item_nameplate_padding", "grid_item_nameplate_radius", "grid_item_nameplate_inner_radius", "grid_container_padding_left", "grid_container_padding_top", "button_container_size", "selection_stroke_size", "grid_scale_min_m3", "grid_scale_max_m3"]),
      },
  chipsAndSort: {
    ...group(["search_chip_group_padding_vertical", "search_chip_inactive_stroke_width", "bottom_sheet_dialog_radius", "bottom_sheet_dialog_width", "bottom_sheet_sort_item_height", "bottom_sheet_sort_item_padding_horizontal", "bottom_sheet_sort_item_padding_vertical", "bottom_sheet_dialog_padding_top", "bottom_sheet_dialog_padding_bottom"]),
    chipStyleParent: DUI(`${M3}/values/styles_m3.xml`, /name="SearchChipItemStyle" parent="@style\/([^"]+)"/),
    chipSelectedFill: DUI(`${M3}/color/search_chip_background_color_m3.xml`, /android:color="([^"]+)" android:state_selected="true"/),
    sheetFill: DUI(`${M3}/drawable/bottom_sheet_dialog_background_m3.xml`, /<solid android:color="([^"]+)"/),
    fileTypeChips: ["chip_title_images", "chip_title_audio", "chip_title_videos", "chip_title_documents"].map(str),
    lastModifiedOptions: ["search_last_modified_any_time", "search_last_modified_1_day", "search_last_modified_7_days", "search_last_modified_30_days", "search_last_modified_365_days"].map(str),
    sortDimensions: ["sort_dimension_name", "sort_dimension_summary", "sort_dimension_file_type", "sort_dimension_size", "sort_dimension_date"].map(str),
  },
  inspectorAndPeek: {
    ...group(["peek_metadata_sheet_width", "peek_metadata_bottom_sheet_collapsed_height", "peek_metadata_bottom_sheet_expanded_offset", "inspector_header_height_m3"]),
    peekScrim: C("peek_overlay_scrim"),
    sheetSurface: DUI(`${M3}/values/themes_m3.xml`, /name="android:backgroundTint">\s*\?attr\/(\w+)/),
    labelStyle: xmlEl(`${M3}/layout/get_info_item_m3.xml`, "@+id/item_label", "textAppearance"),
    valueStyle: xmlEl(`${M3}/layout/get_info_item_m3.xml`, "@+id/item_value", "textAppearance"),
  },
  emptyStates: {
    ...group(["empty_state_artwork_margin_top", "empty_state_artwork_margin_bottom", "empty_state_message_margin_bottom"]),
    artworkSize: DUI(`${M3}/drawable/empty_m3.xml`, /android:width="([^"]+)"/),
    messageStyle: DUI(`${M3}/values/styles_text_m3.xml`, /name="EmptyStateTitleTextM3" parent="@style\/TextAppearance\.DocumentsUI\.([A-Za-z.]+)"/),
    emptyString: str("empty"),
    noResultsString: strM3("no_results_m3"),
  },
  banner: group(["banner_content_padding_vertical", "banner_content_padding_start", "banner_content_padding_end", "banner_message_icon_size"]),
  jobPanel: group(["job_progress_panel_width", "job_progress_panel_margin", "job_progress_list_corner_radius"]),
  dragAndDrop: group(["drag_content_width", "drag_content_height", "drag_content_radius", "drag_shadow_width_m3", "drag_shadow_height_m3"]),
  dialog: group(["dialog_max_width", "dialog_content_padding_horizontal"]),
  theme: {
    parent: DUI(`${M3}/values/themes_m3.xml`, /name="DocumentsDefaultThemeM3" parent="@style\/([^"]+)"/),
    flag: DUI("flags.aconfig", /name: "(use_material3)"/),
    fontFamily: DUI("res/values/strings_font.xml", /name="config_fontFamily" translatable="false">([^<]+)</),
    fontFamilyMedium: DUI("res/values/strings_font.xml", /name="config_fontFamilyMedium" translatable="false">([^<]+)</),
  },
  spacing: spacing(),
  strings: group(["menu_open", "menu_open_with", "menu_share", "menu_cut_to_clipboard", "menu_copy_to_clipboard", "menu_paste_from_clipboard", "menu_paste_into_folder", "menu_copy", "menu_move", "menu_inspect", "menu_rename", "menu_move_to_trash", "menu_restore_from_trash", "menu_permanently_delete", "menu_compress", "menu_extract", "menu_search", "menu_grid", "menu_list", "menu_new_window", "menu_create_dir", "menu_sort", "menu_select_all", "menu_deselect_all", "menu_show_hidden_files", "menu_settings", "root_recent", "root_trash", "search_bar_hint", "search_bar_generic_hint", "bullet"], str),
  keyboard: [
    keyRow("Ctrl+Space", "toggle focused item selection", "files/FilesActivity.java", /case KeyEvent\.KEYCODE_SPACE:\s*\/\/ Disable ctrl\+space/),
    keyRow("Ctrl+A", "select all", "files/FilesActivity.java", /case KeyEvent\.KEYCODE_A:/),
    keyRow("Ctrl+X", "cut", "files/FilesActivity.java", /case KeyEvent\.KEYCODE_X:/),
    keyRow("Ctrl+C", "copy", "files/FilesActivity.java", /case KeyEvent\.KEYCODE_C:/),
    keyRow("Ctrl+V", "paste", "files/FilesActivity.java", /case KeyEvent\.KEYCODE_V:/),
    keyRow("Ctrl+R", "refresh", "files/FilesActivity.java", /case KeyEvent\.KEYCODE_R:/),
    keyRow("Ctrl+Enter", "rename the single selected item", "files/FilesActivity.java", /case KeyEvent\.KEYCODE_ENTER:\s*\{\s*if \(!isUseMaterial3FlagEnabled\(\)\) break;/),
    keyRow("F5 (KEYCODE_REFRESH)", "refresh", "files/FilesActivity.java", /keyCode == KEYCODE_REFRESH/),
    keyRow("Alt+Backspace / Delete", "move to trash or delete", "files/ActivityInputHandler.java", /case KeyEvent\.KEYCODE_FORWARD_DEL:/),
    keyRow("Esc", "clear selection or go back", "SharedInputHandler.java", /case KeyEvent\.KEYCODE_ESCAPE:/),
    keyRow("Tab", "move focus between roots and directory", "SharedInputHandler.java", /case KeyEvent\.KEYCODE_TAB:/),
    keyRow("Search key", "open search", "SharedInputHandler.java", /case KeyEvent\.KEYCODE_SEARCH:/),
    keyRow("Space (in list)", "toggle selection of the focused item", "dirlist/InputHandlers.java", /case KeyEvent\.KEYCODE_SPACE:/),
    keyRow("Enter (in list)", "open the focused item", "dirlist/InputHandlers.java", /case KeyEvent\.KEYCODE_ENTER:/),
    keyRow("Home/End/PageUp/PageDown/arrows", "list navigation", "base/Events.java", /case KeyEvent\.KEYCODE_MOVE_HOME:/),
  ],
};

function table(kind: string) {
  const names = new Set<string>();
  for (const r of allRes) if ((r.kind === kind || (kind === "dimen" && (r.kind === "integer" || r.kind === "fraction"))) && r.file.startsWith(M3)) names.add(r.name);
  return Object.fromEntries([...names].sort().map((n) => [n, kind === "dimen" ? T(n) : tokenOf(kind, n)]));
}
const textStyles = (() => {
  const f = `${M3}/values/styles_text_m3.xml`;
  const s = read(join(R.dui, f));
  return [...s.matchAll(/<style name="([^"]+)"(?: parent="@?(?:style\/)?([^"]*)")?\s*(?:\/>|>([\s\S]*?)<\/style>)/g)].map((m) => ({
    name: m[1],
    parent: m[2] ?? null,
    color: m[3]?.match(/textColor">([^<]+)</)?.[1] ?? null,
    fontFamily: m[3]?.match(/"fontFamily">([^<]+)</)?.[1] ?? null,
    src: duiSrc(f, lineAt(s, m.index!)),
  }));
})();
const typescale = (() => {
  const s = read(R.mc);
  const out: Record<string, unknown> = {};
  for (const m of s.matchAll(/<style name="TextAppearance\.M3\.Sys\.Typescale\.(\w+)"[^>]*>([\s\S]*?)<\/style>/g)) {
    const g = (k: string) => m[2].match(new RegExp(`name="${k}">([^<]+)<`))?.[1];
    out[m[1]] = { size: g("android:textSize"), lineHeight: g("android:lineHeight"), letterSpacing: g("android:letterSpacing"), typeface: g("android:fontFamily"), src: `material-components-android:lib/java/com/google/android/material/typography/res/values/tokens.xml:${lineAt(s, m.index!)}` };
  }
  return out;
})();
const menus = (() => {
  const out: Record<string, unknown[]> = {};
  for (const f of ["activity_m3.xml", "action_mode_menu_m3.xml", "dir_context_menu_m3.xml", "file_context_menu_m3.xml", "container_context_menu_m3.xml", "root_context_menu_m3.xml"]) {
    const rel = `${M3}/menu/${f}`;
    const s = read(join(R.dui, rel));
    out[f] = [...s.matchAll(/<item\b([^>]*)>/g)].map((m) => {
      const a = (k: string) => m[1].match(new RegExp(`(?:android|app):${k}="([^"]+)"`))?.[1] ?? null;
      return { id: a("id"), title: a("title"), icon: a("icon"), showAsAction: a("showAsAction"), src: duiSrc(rel, lineAt(s, m.index!)) };
    });
  }
  return out;
})();

// ---------- ChromeOS Files ----------
const crSrc = (f: string, line: number) => `chromium-file-manager:${norm(f)}:${line}`;
const CRF = "ui/file_manager/file_manager";
const CSS = `${CRF}/foreground/css/file_manager.css`;
const QV = `${CRF}/foreground/elements/files_quick_view.css`;
const TREE = `${CRF}/widgets/xf_tree_item.ts`;
const CR = (f: string, re: RegExp) => reFact(R.cr, "chromium-file-manager", f, re);

function cssRules(file: string) {
  const s = read(join(R.cr, file));
  const clean = s.replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, " "));
  const rules: { sels: string[]; body: string; start: number }[] = [];
  for (const m of clean.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const sels = m[1].split(",").map((x) => x.replace(/\s+/g, " ").trim());
    rules.push({ sels, body: m[2], start: lineAt(clean, m.index! + m[0].indexOf("{") + 1) });
  }
  return rules;
}
function css(file: string, sel: string, prop: string, occ = 0) {
  const hits: { value: string; src: string }[] = [];
  for (const r of cssRules(file)) {
    if (!r.sels.includes(sel)) continue;
    r.body.split("\n").forEach((l, i) => {
      const m = l.trim().match(new RegExp(`^${prop.replace(/-/g, "\\-")}\\s*:\\s*(.+?);?$`));
      if (m) hits.push({ value: m[1].replace(/;$/, ""), src: crSrc(file, r.start + i) });
    });
  }
  return hits[occ] ?? fail(`css ${file} ${sel} ${prop} #${occ}`);
}
const refColors = Bun.JSON5.parse(read(join(R.cr, "ui/chromeos/styles/cros_ref_colors.json5"))).colors as Record<string, string>;
const sysFile = "ui/chromeos/styles/cros_sys_colors.json5";
const sysRaw = read(join(R.cr, sysFile));
const sys = Bun.JSON5.parse(sysRaw);
const keyLine = (raw: string, key: string) => {
  const i = raw.search(new RegExp(`^\\s*['"]?${key.replace(/-/g, "\\-")}['"]?\\s*:\\s*\\{`, "m"));
  return i < 0 ? 0 : lineAt(raw, i + 1);
};
const subRef = (v: string) =>
  v.replace(/\$cros\.ref\.([a-z0-9]+)(\.rgb)?/gi, (_m, n: string, rgb?: string) => {
    const hex = refColors[n] ?? `?${n}`;
    if (!rgb) return hex;
    const h = hex.replace("#", "");
    return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)).join(", ");
  });
const crosSys = Object.fromEntries(
  Object.entries<any>(sys.colors).map(([k, v]) => [k.replaceAll("-", "_"), { light: v.light ? subRef(v.light) : null, dark: v.dark ? subRef(v.dark) : null, lightRef: v.light ?? null, darkRef: v.dark ?? null, src: crSrc(sysFile, keyLine(sysRaw, k)) }]),
);
const typo = (f: string) => {
  const s = read(join(R.cr, f));
  const j = Bun.JSON5.parse(s).typography;
  const fam = j.font_families ?? {};
  return Object.fromEntries(Object.entries<any>(j.typefaces).map(([k, v]) => [k, { family: fam[String(v.font_family).slice(1)] ?? v.font_family, size: v.font_size, weight: v.font_weight, lineHeight: v.line_height, src: crSrc(f, keyLine(s, k)) }]));
};
const mainHtml = read(join(R.cr, `${CRF}/main.html`));
const commands = [...mainHtml.matchAll(/<command\b([^>]*)>/g)].map((m) => ({
  id: m[1].match(/id="([^"]+)"/)![1],
  shortcut: m[1].match(/shortcut="([^"]*)"/)?.[1] ?? null,
  label: m[1].match(/label="([^"]*)"/)?.[1] ?? null,
  src: crSrc(`${CRF}/main.html`, lineAt(mainHtml, m.index!)),
}));
const toolbarButtons = [...mainHtml.matchAll(/<cr-button id="([^"]+)" class="([^"]*)"/g)].map((m) => ({ id: m[1], cls: m[2], src: crSrc(`${CRF}/main.html`, lineAt(mainHtml, m.index!)) }));
const sortMenu = [...mainHtml.matchAll(/<cr-menu-item id="(sort-menu-[^"]+)"/g)].map((m) => ({ id: m[1], src: crSrc(`${CRF}/main.html`, lineAt(mainHtml, m.index!)) }));
const columnDefs = read(join(R.cr, `${CRF}/foreground/js/ui/file_table.ts`));
const columns = [...columnDefs.matchAll(/new TableColumn\(\s*'(\w+)',\s*str\('(\w+)'\),\s*(fullPage \? \d+ : \d+|\d+)/g)].map((m) => ({ id: m[1], label: m[2], widthPx: m[3], src: crSrc(`${CRF}/foreground/js/ui/file_table.ts`, lineAt(columnDefs, m.index!)) }));

const chrome = {
  commit: CR("README.md", /commit ([0-9a-f]{40})/),
  layout: {
    navigationPaneWidth: css(CSS, ".dialog-navigation-list", "width"),
    navigationPaneMin: css(CSS, ".dialog-navigation-list", "min-width"),
    navigationPaneMax: css(CSS, ".dialog-navigation-list", "max-width"),
    dialogContainerPaddingTop: css(CSS, ".dialog-container", "padding-top"),
    dialogContainerPaddingEnd: css(CSS, ".dialog-container", "padding-inline-end"),
    toolbarHeight: css(CSS, ".dialog-header", "height"),
    toolbarBorder: css(CSS, ".dialog-header", "border-bottom"),
    toolbarColor: css(CSS, ".dialog-header", "color"),
    toolbarButtonHeight: css(CSS, ".dialog-header cr-button", "height"),
    toolbarButtonRadius: css(CSS, ".dialog-header cr-button", "border-radius"),
    toolbarButtonMinWidth: css(CSS, ".dialog-header cr-button", "min-width"),
    toolbarButtonHitArea: css(CSS, ".dialog-header cr-button::after", "height"),
    toolbarIconButtonSize: css(CSS, ".dialog-header cr-button.icon-button", "width"),
    toolbarIconButtonRadius: css(CSS, ".dialog-header cr-button.icon-button", "border-radius"),
    toolbarIconSize: css(CSS, ".dialog-header cr-button.icon-button > .icon", "width"),
    toolbarMenuShownFill: css(CSS, ".dialog-header cr-button[menu-shown]", "background-color"),
    searchHeight: css(CSS, ".dialog-header.files-ng #search-wrapper", "height"),
    searchRadius: css(CSS, ".dialog-header.files-ng #search-wrapper", "border-radius"),
    searchFont: css(CSS, "#search-box", "font"),
    selectionCancelSize: css(CSS, "body.files-ng #cancel-selection-button", "width"),
    breadcrumbPaddingY: css(CSS, ".breadcrumbs", "padding-top"),
    quickViewToolbarHeight: CR(QV, /#toolbar \{[^}]*?height: (\d+px)/s),
    quickViewToolbarFont: CR(QV, /#toolbar \{[^}]*?font: ([^;]+);/s),
    metadataPanelWidth: CR(QV, /#contentPanel\[metadata-box-active\] \{\s*margin-inline-end: (\d+px)/),
    metadataPanelFill: CR(QV, /#metadata-box \{\s*background-color: ([^;]+);/),
    quickViewScrim: CR(QV, /#innerContentPanel \{\s*background-color: ([^;]+);/),
  },
  tree: {
    indent: CR(TREE, /export const TREE_ITEM_INDENT = (\d+)/),
    rowHeight: CR(TREE, /\.tree-row \{[^}]*?height: (\d+px)/s),
    rowRadius: CR(TREE, /\.tree-row \{[^}]*?border-radius: (\d+px)/s),
    rowWrapperPadding: CR(TREE, /\.tree-row-wrapper \{[^}]*?padding: (\d+px)/s),
    rowPaddingEnd: CR(TREE, /\.tree-row \{[^}]*?padding-inline-end: (\d+px)/s),
    rowColor: CR(TREE, /\.tree-row \{[^}]*?color: ([^;]+);/s),
  },
  table: {
    rowHeight: css(CSS, "#list-container li.table-row", "height"),
    rowFont: css(CSS, "#list-container li.table-row", "font"),
    rowFill: css(CSS, "#list-container li.table-row", "background-color"),
    headerHeight: css(CSS, ".table-header", "height"),
    headerLabelFont: css(CSS, ".table-header-label", "font"),
    headerBorder: css(CSS, ".table-header", "border-bottom"),
    cellPadding: css(CSS, ".table-row-cell > *", "padding"),
    detailIcon: css(CSS, ".detail-icon", "width"),
    detailThumbnailRadius: css(CSS, "#list-container list li .detail-thumbnail > .thumbnail", "border-radius", 0),
    checkmark: css(CSS, "body.files-ng #list-container li .detail-checkmark", "width"),
    groupHeadingHeight: css(CSS, "#list-container .group-heading", "height"),
    columns,
  },
  grid: {
    itemWidth: css(CSS, ".thumbnail-grid .thumbnail-item", "width"),
    itemHeight: css(CSS, ".thumbnail-grid .thumbnail-item", "height"),
    itemRadius: css(CSS, ".thumbnail-grid .thumbnail-item", "border-radius"),
    itemGap: css(CSS, ".thumbnail-grid .thumbnail-item", "margin-inline-start"),
    imageHeight: css(CSS, ".thumbnail-grid .img-container", "height"),
    labelBandHeight: css(CSS, ".thumbnail-grid .thumbnail-bottom", "height"),
    folderTileHeight: css(CSS, ".thumbnail-item.directory", "height"),
    gridInsetX: css(CSS, ".thumbnail-grid", "padding-inline"),
    dragSelectionRadius: css(CSS, "#file-list .drag-selection-border", "border-radius"),
  },
  stateColors: {
    hover: css(CSS, ".dialog-header cr-button", "--hover-bg-color"),
    ripple: css(CSS, ".dialog-header cr-button", "--ink-color"),
    pressed: css(CSS, ".dialog-header cr-button:not([aria-haspopup]):active", "background-color"),
    focusRing: css(CSS, ".dialog-header.files-ng cr-toggle:focus".replace(":focus", ":focus").length ? "html.focus-outline-visible .dialog-header.files-ng cr-toggle:focus" : "", "--cr-toggle-ripple-ring"),
    selectedRow: css(CSS, "body.files-ng #list-container list > li[selected]", "background-color"),
    selectedRowListFocused: css(CSS, "body.files-ng #list-container list:focus > li[selected]", "background-color"),
    selectionBand: css(CSS, "#file-list .drag-selection-border", "background-color"),
    searchFieldFill: css(CSS, ".dialog-header.files-ng #search-wrapper.has-cursor, .dialog-header.files-ng #search-wrapper.has-text, .dialog-header.files-ng #search-wrapper:hover".split(",")[0].trim(), "background-color"),
  },
  commands,
  toolbarButtons,
  sortMenu,
  tokens: { sys: crosSys, typographyCros: typo("ui/chromeos/styles/cros_typography.json5"), typographyGm3: typo("ui/chromeos/styles/cros_gm3_typography.json5"), refPalette: refColors },
};

// ---------- Spacedrive ----------
const SD = (f: string, re: RegExp) => reFact(R.sd, "spacedrive", f, re);
const crates = readdirSync(join(R.sd, "crates"), { withFileTypes: true })
  .filter((d) => d.isDirectory())
  .map((d) => {
    const toml = join(R.sd, "crates", d.name, "Cargo.toml");
    const t = existsSync(toml) ? read(toml) : "";
    const lib = join(R.sd, "crates", d.name, "src/lib.rs");
    const doc = existsSync(lib) ? read(lib).split("\n").filter((l) => l.startsWith("//!")).slice(0, 2).map((l) => l.replace(/^\/\/!\s?/, "")).join(" ").trim() : "";
    return { crate: d.name, package: t.match(/^name\s*=\s*"([^"]+)"/m)?.[1] ?? null, doc };
  });
const kbFile = "packages/interface/src/util/keybinds/registry.ts";
const kb = read(join(R.sd, kbFile));
const keybinds = [...kb.matchAll(/id: '(explorer\.[^']+)',\s*label: '([^']+)',\s*combo: \{([\s\S]*?)\}\s*,?\s*(?:scope|\}\))/g)].map((m) => ({ id: m[1], label: m[2], combo: m[3].replace(/\s+/g, " ").trim(), src: `spacedrive:${kbFile}:${lineAt(kb, m.index!)}` }));
if (keybinds.length < 15) fail(`keybinds ${keybinds.length}`);
const CI = "core/src/domain/content_identity.rs";
const DOCS = "docs/core";
const spacedrive = {
  license: { workspace: SD("Cargo.toml", /license\s*=\s*"([^"]+)"/), notice: SD("NOTICE", /(Releases prior to [^\n]+)/), file: SD("LICENSE", /(Apache License)/) },
  identity: {
    minimumFileSize: SD(CI, /MINIMUM_FILE_SIZE: u64 = ([^;]+);/),
    sampleCount: SD(CI, /SAMPLE_COUNT: u64 = ([^;]+);/),
    sampleSize: SD(CI, /SAMPLE_SIZE: u64 = ([^;]+);/),
    headerFooterSize: SD(CI, /HEADER_OR_FOOTER_SIZE: u64 = ([^;]+);/),
    sampledHashFn: SD(CI, /(pub async fn generate_content_hash)\b/),
    integrityHashFn: SD(CI, /(pub async fn generate_integrity_hash)\b/),
    unknown: SD(`${DOCS}/data-model.mdx`, /(\| Unknown \|[^\n]*)/),
    candidate: SD(`${DOCS}/data-model.mdx`, /(\| Candidate \|[^\n]*)/),
    confirmed: SD(`${DOCS}/data-model.mdx`, /(\| Confirmed \|[^\n]*)/),
    recordTable: SD("crates/store/src/record.rs", /(The universal record table shared by every source index)/),
    overlay: SD("crates/store/src/record.rs", /(`record_overlay` sits beside them[^\n]*)/),
  },
  indexer: {
    job: SD("core/src/ops/indexing/job.rs", /(pub struct IndexerJob)\b/),
    discovery: SD("core/src/ops/indexing/phases/mod.rs", /(pub mod discovery)/),
    watcherCrate: SD("crates/fs-watcher/src/lib.rs", /(Platform-agnostic filesystem watcher)/),
    watchers: SD(`${DOCS}/indexing.mdx`, /(After the initial walk, filesystem watchers[^\n]*)/),
    contentId: SD(`${DOCS}/indexing.mdx`, /(Normal identification reads[^\n]*)/),
  },
  jobs: {
    trait: SD("core/src/infra/job/traits.rs", /(pub trait Job: [^{]+)\{/),
    handler: SD("core/src/infra/job/traits.rs", /(pub trait JobHandler: Job)/),
    dedup: SD(`${DOCS}/jobs.mdx`, /(Dispatch deduplicates by job kind[^\n]*)/),
    taskSystem: SD("crates/task-system/src/lib.rs", /(Spacedrive's Task System[^\n]*)/),
  },
  thumbnails: {
    hotTier: SD("crates/pvcache/src/lib.rs", /(Memory-mapped thumbnail tile cache[^\n]*)/),
    producers: SD("crates/bake/src/lib.rs", /(A \[`Producer`\] is one stage of a fall-through chain[^\n]*)/),
    batch: SD(`${DOCS}/thumbnail-generation.mdx`, /(The job submits up to \d+ files at a time[^\n]*)/),
    cache: SD(`${DOCS}/thumbnail-generation.mdx`, /(Hot tiles live in `[^`]+`)/),
  },
  tags: {
    definitions: SD(`${DOCS}/tagging.mdx`, /(A definition is the tag itself[^\n]*)/),
    assertions: SD(`${DOCS}/tagging.mdx`, /(An application is the claim that a definition applies[^\n]*)/),
    order: SD(`${DOCS}/tagging.mdx`, /(The state of a tag on a record is the latest assertion by HLC[^\n]*)/),
  },
  sync: {
    planes: SD(`${DOCS}/library-sync.mdx`, /(\| Library \| Devices, registry[^\n]*)/),
    hlc: SD("core/src/infra/sync/hlc.rs", /(pub struct HLC)\b/),
    notFileSync: SD(`${DOCS}/library-sync.mdx`, /(## Library sync is not file sync)/),
  },
  ui: {
    viewModes: SD("packages/interface/src/routes/explorer/context.tsx", /(\| 'grid'\s*\| 'list'\s*\| 'media'\s*\| 'column'\s*\| 'size'\s*\| 'knowledge')/),
    sidebarWidth: SD("packages/interface/src/contexts/ShellGeometryContext.tsx", /SHELL_SIDEBAR_WIDTH = (\d+)/),
    inspectorWidth: SD("packages/interface/src/contexts/ShellGeometryContext.tsx", /SHELL_INSPECTOR_WIDTH = (\d+)/),
    rowHeight: SD("packages/interface/src/routes/explorer/views/ListView/useTable.tsx", /ROW_HEIGHT = (\d+)/),
    tableHeaderHeight: SD("packages/interface/src/routes/explorer/views/ListView/useTable.tsx", /TABLE_HEADER_HEIGHT = (\d+)/),
    tablePaddingY: SD("packages/interface/src/routes/explorer/views/ListView/useTable.tsx", /TABLE_PADDING_Y = (\d+)/),
    gridSizeDefault: SD("packages/interface/src/routes/explorer/context.tsx", /gridSize: (\d+),/),
    gapSizeDefault: SD("packages/interface/src/routes/explorer/context.tsx", /gapSize: (\d+),/),
    columnWidthDefault: SD("packages/interface/src/routes/explorer/context.tsx", /columnWidth: (\d+),/),
    gridTemplate: SD("packages/interface/src/routes/explorer/views/GridView/GridView.tsx", /gridTemplateColumns: (`[^`]+`)/),
    mediaGap: SD("packages/interface/src/routes/explorer/views/MediaView/MediaView.tsx", /(Fixed 1px gap for Apple Photos style)/),
    nativeRadiusWindow: SD("apps/native/src/theme.rs", /radius_window: px\(([\d.]+)\)/),
    nativeRadiusLg: SD("apps/native/src/theme.rs", /radius_lg: px\(([\d.]+)\)/),
    nativeAccent: SD("apps/native/src/theme.rs", /accent: hsl\(([^)]+)\)/),
    nativeApp: SD("apps/native/src/theme.rs", /\bapp: hsl\(([^)]+)\)/),
    nativeSidebar: SD("apps/native/src/theme.rs", /\bsidebar: hsl\(([^)]+)\)/),
    nativeSelected: SD("apps/native/src/theme.rs", /app_selected: hsl\(([^)]+)\)/),
  },
  keybinds,
  crates,
};

const out = {
  generatedBy: "crates/fw-drive/replica/tools/extract-upstream-tokens.ts",
  note: "Every leaf with a src field was read from the pinned upstream checkout at generation time. Android sizes are dp/sp, Chrome sizes are CSS px, Spacedrive UI sizes are CSS px.",
  upstreams: PINS,
  android: { ...android, tokens: { dimens: table("dimen"), colors: table("color") }, textStyles, typescale, menus },
  chromeos: chrome,
  spacedrive,
};
writeFileSync(OUT, JSON.stringify(out, null, 1) + "\n");
console.log(`wrote ${OUT} (${(JSON.stringify(out).length / 1024).toFixed(0)} KiB)`);
