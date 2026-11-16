// SPDX-License-Identifier: Apache-2.0
//! The character dataset parses, every fact has a source that resolves to a URL and a retrieval date,
//! the files it points at (SVGs, rasters, theme tokens, measurements, index) agree with it, the names
//! agree with `ay.toml`, and the data issues of `merge.json` are resolved or listed as open.

use ay_brand::character::{Character, INDEX_MD, Report, VOICE_JSON};
use ay_brand::sha256_hex;
use serde_json::Value;
use std::path::PathBuf;

/// The repository root: the crate lives in `crates/ay_brand`.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn json(path: &str) -> Value {
    let text = std::fs::read_to_string(root().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn report() -> (Character, Report) {
    let c = Character::bundled().expect("the dataset parses");
    let r = c
        .validate()
        .unwrap_or_else(|bad| panic!("dataset rules violated:\n{}", bad.join("\n")));
    (c, r)
}

fn fact<'a>(c: &'a Character, id: &str) -> &'a Value {
    &c.canon
        .facts
        .iter()
        .find(|f| f.id == id)
        .unwrap_or_else(|| panic!("fact {id}"))
        .value
}

#[test]
fn the_dataset_parses_and_every_item_has_a_resolving_source() {
    let (c, r) = report();
    println!(
        "facts={} colours={} fonts={} svgs={} images={} sources={} conflicts_resolved={} conflicts_open={}",
        r.facts,
        r.colours,
        r.fonts,
        r.svgs,
        r.images,
        r.sources,
        r.conflicts_resolved,
        r.conflicts_open
    );
    assert!(r.facts >= 100, "facts: {}", r.facts);
    assert!(r.colours >= 20, "colours: {}", r.colours);
    assert!(r.fonts >= 2 && r.svgs >= 3 && r.images >= 10);
    assert!(c.canon.facts.iter().all(|f| !f.sources.is_empty()));
}

#[test]
fn every_fact_cites_a_source_with_a_url_and_a_date() {
    let c = Character::bundled().unwrap();
    for f in &c.canon.facts {
        for id in &f.sources {
            let s = c
                .sources
                .sources
                .iter()
                .find(|s| &s.id == id)
                .unwrap_or_else(|| panic!("{}: unknown source {id}", f.id));
            assert!(s.url.starts_with("https://"), "{}: {id} has no URL", f.id);
            assert_eq!(
                s.retrieved.len(),
                10,
                "{}: {id} has no retrieval date",
                f.id
            );
        }
    }
}

#[test]
fn canon_covers_ja_en_fr_names_teams_and_techniques() {
    let c = Character::bundled().unwrap();
    for lang in ["ja", "en", "fr"] {
        assert!(
            c.canon.languages.iter().any(|l| l == lang),
            "language {lang}"
        );
        assert!(
            c.canon
                .facts
                .iter()
                .any(|f| f.category == "name" && f.id.contains(&format!(".{lang}."))),
            "name fact for {lang}"
        );
    }
    for cat in [
        "team",
        "position",
        "element",
        "technique",
        "timeline",
        "appearance",
        "stat",
    ] {
        assert!(
            c.canon.facts.iter().any(|f| f.category == cat),
            "category {cat}"
        );
    }
    assert!(
        c.canon
            .facts
            .iter()
            .filter(|f| f.category == "technique")
            .count()
            >= 10
    );
}

#[test]
fn every_rule_has_a_source_but_a_dangling_one_is_refused() {
    let mut c = Character::bundled().unwrap();
    c.canon.facts[0].sources.clear();
    c.canon.facts[1].sources = vec!["no-such-source".to_owned()];
    let bad = c
        .validate()
        .expect_err("sourceless and dangling facts are refused");
    assert!(bad.iter().any(|m| m.contains("has no source")));
    assert!(
        bad.iter()
            .any(|m| m.contains("unknown source no-such-source"))
    );
}

#[test]
fn a_conflict_without_a_status_or_a_second_canonical_seed_is_refused() {
    let mut c = Character::bundled().unwrap();
    c.merge["conflicts"][0]["status"] = Value::from("maybe");
    c.visual["palette"]["seeds"][1]["status"] = Value::from("canonical");
    let bad = c
        .validate()
        .expect_err("an unknown status and two canonical seeds are refused");
    assert!(
        bad.iter()
            .any(|m| m.contains("is neither resolved nor open")),
        "{bad:?}"
    );
    assert!(
        bad.iter().any(|m| m.contains("2 canonical seeds")),
        "{bad:?}"
    );
}

#[test]
fn the_svgs_exist_and_match_their_recorded_hash_and_size() {
    let (c, _) = report();
    for svg in c.visual["svgs"].as_array().unwrap() {
        let path = root().join(svg["path"].as_str().unwrap());
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            bytes.len() as u64,
            svg["bytes"].as_u64().unwrap(),
            "{}",
            path.display()
        );
        assert_eq!(
            sha256_hex(&bytes),
            svg["sha256"].as_str().unwrap(),
            "{}",
            path.display()
        );
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("<svg") && text.contains(svg["viewBox"].as_str().unwrap()));
    }
}

#[test]
fn every_recorded_raster_and_image_file_exists_with_its_hash_and_size() {
    let (c, _) = report();
    let mut checked = 0;
    let rasters = c.visual["raster_assets"].as_array().unwrap();
    let images = c.visual["images"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i.get("path").is_some());
    for item in rasters.iter().chain(images) {
        let path = root().join(item["path"].as_str().unwrap());
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            bytes.len() as u64,
            item["bytes"].as_u64().unwrap(),
            "{}",
            path.display()
        );
        assert_eq!(
            sha256_hex(&bytes),
            item["sha256"].as_str().unwrap(),
            "{}",
            path.display()
        );
        checked += 1;
    }
    assert!(checked >= 15, "checked {checked} files");
}

#[test]
fn the_theme_tokens_equal_the_brand_token_file() {
    let (c, _) = report();
    let tokens = json("brand/tokens/m3-tokens.json");
    for mode in ["light", "dark"] {
        for (k, v) in c.visual["theme"][mode].as_object().unwrap() {
            assert_eq!(&tokens[mode][k], v, "{mode}.{k}");
        }
    }
    assert_eq!(c.visual["theme"]["seed"], tokens["seed"]);
}

#[test]
fn the_canonical_seed_is_the_one_the_token_files_are_built_from() {
    let (c, _) = report();
    let tokens = json("brand/tokens/m3-tokens.json");
    let seed = tokens["seed"].as_str().unwrap().to_ascii_uppercase();
    let canonical: Vec<&Value> = c.visual["palette"]["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["status"] == "canonical")
        .collect();
    assert_eq!(canonical.len(), 1);
    assert_eq!(canonical[0]["hex"].as_str().unwrap(), seed);
    let css = std::fs::read_to_string(root().join("brand/tokens/m3-tokens.css")).unwrap();
    assert!(
        css.contains(&format!("seed {}", seed.to_ascii_lowercase())),
        "the CSS header names another seed"
    );
    for (var, key) in [
        ("--md-sys-color-primary", "primary"),
        ("--md-sys-color-surface", "surface"),
    ] {
        assert!(
            css.contains(&format!(
                "{var}: {};",
                tokens["light"][key].as_str().unwrap()
            )),
            "{var} differs from the JSON tokens"
        );
    }
}

#[test]
fn the_token_files_are_the_material_tonal_spot_scheme_of_the_canonical_seed() {
    use material_colors::color::Rgb;
    use material_colors::dynamic_color::{SpecVersion, Variant};
    use material_colors::theme::ThemeBuilder;

    let tokens = json("brand/tokens/m3-tokens.json");
    assert_eq!(tokens["variant"], "tonal-spot");
    assert_eq!(tokens["spec_version"], "2021");
    let seed: Rgb = tokens["seed"]
        .as_str()
        .unwrap()
        .parse()
        .expect("the seed is a hex colour");
    let theme = ThemeBuilder::with_source(seed)
        .variant(Variant::TonalSpot)
        .spec_version(SpecVersion::Spec2021)
        .contrast_level(0.0)
        .build();
    for (mode, s) in [
        ("light", &theme.schemes.light),
        ("dark", &theme.schemes.dark),
    ] {
        let roles = [
            ("primary", s.primary),
            ("onPrimary", s.on_primary),
            ("primaryContainer", s.primary_container),
            ("onPrimaryContainer", s.on_primary_container),
            ("secondary", s.secondary),
            ("tertiary", s.tertiary),
            ("background", s.background),
            ("surface", s.surface),
            ("onSurface", s.on_surface),
            ("outline", s.outline),
            ("error", s.error),
        ];
        for (name, rgb) in roles {
            assert_eq!(
                tokens[mode][name],
                format!("#{}", rgb.as_hex()),
                "{mode}.{name} is not what the seed generates"
            );
        }
    }
}

#[test]
fn the_webmanifest_takes_its_names_and_colours_from_ay_toml_and_the_tokens() {
    let ay: toml::Table = std::fs::read_to_string(root().join("ay.toml"))
        .unwrap()
        .parse()
        .unwrap();
    let display = ay["product"]["display"].as_str().unwrap();
    let manifest = json("brand/assets/site.webmanifest");
    assert_eq!(manifest["name"], display);
    assert_eq!(manifest["short_name"], display);
    let tokens = json("brand/tokens/m3-tokens.json");
    assert_eq!(manifest["theme_color"], tokens["light"]["primary"]);
    assert_eq!(manifest["background_color"], tokens["light"]["background"]);
    for icon in manifest["icons"].as_array().unwrap() {
        assert!(
            root()
                .join("brand/assets")
                .join(icon["src"].as_str().unwrap())
                .is_file(),
            "{}",
            icon["src"]
        );
    }
    let (c, _) = report();
    assert!(
        fact(&c, "name.composite")["name_en"]
            .as_str()
            .unwrap()
            .contains(display),
        "the canon composite name lacks the product name"
    );
}

#[test]
fn the_measured_colours_match_the_measurement_files() {
    let (c, _) = report();
    let m = json("brand/character/measurements/portraits-k12.json");
    let palette = &c.visual["palette"];
    let hexes: Vec<&str> = m["clusters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["hex"].as_str().unwrap())
        .collect();
    let got: Vec<&str> = palette["clusters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["hex"].as_str().unwrap())
        .collect();
    assert_eq!(hexes, got);
    for (role, v) in m["roles"].as_object().unwrap() {
        let r = palette["roles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == format!("role.{role}"))
            .expect("role present");
        assert_eq!(r["hex"], v["hex"], "role {role}");
    }
    assert_eq!(palette["element_accents"].as_array().unwrap().len(), 3);
}

#[test]
fn every_font_has_a_licence_field_and_the_known_ones_are_verified() {
    let (c, _) = report();
    for f in c.visual["fonts"].as_array().unwrap() {
        assert!(f.get("licence").is_some(), "{}", f["name"]);
        assert!(
            f["licence_status"].as_str().is_some_and(|s| !s.is_empty()),
            "{}",
            f["name"]
        );
    }
    let gsf = c.visual["fonts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "Google Sans Flex")
        .unwrap();
    assert_eq!(gsf["licence"], "SIL OFL 1.1");
}

#[test]
fn the_index_is_current() {
    let (_, r) = report();
    for needle in [
        format!("Facts: {}", r.facts),
        format!("Colours: {}", r.colours),
        format!("Fonts: {}", r.fonts),
        format!("SVGs: {}", r.svgs),
        format!("Sources: {}", r.sources),
        format!("{} resolved", r.conflicts_resolved),
    ] {
        assert!(
            INDEX_MD.contains(&needle),
            "index.md is stale: missing '{needle}' (run tools/render_character_index.py)"
        );
    }
}

#[test]
fn the_voice_samples_in_other_languages_come_from_sources() {
    let c = Character::bundled().unwrap();
    let voice: Value = serde_json::from_str(VOICE_JSON).unwrap();
    assert_eq!(voice["source_language"], "en");
    for s in voice["samples"].as_array().unwrap() {
        assert!(
            s["text"]["en"].as_str().is_some_and(|t| !t.is_empty()),
            "{}",
            s["id"]
        );
        if s["origin"] == "authored" {
            assert!(
                s["text"].get("ja").is_none() && s["text"].get("fr").is_none(),
                "authored copy must not carry invented translations: {}",
                s["id"]
            );
            assert!(s["derived_from"].as_array().is_some_and(|d| !d.is_empty()));
        }
    }
    assert!(!c.voice.is_empty());
}

// ---- the data issues of merge.json ----

#[test]
fn the_position_and_element_maps_follow_the_game_files_and_agree_with_the_techniques() {
    let (c, _) = report();
    let positions = fact(&c, "position.id_mapping");
    assert_eq!(
        positions,
        &serde_json::json!({"1": "GK", "2": "FW", "3": "MF", "4": "DF"})
    );
    assert_eq!(fact(&c, "position.primary")["game_main_position_id"], 3);
    assert_eq!(positions["3"], fact(&c, "position.primary")["position"]);
    assert_eq!(
        positions[fact(&c, "position.secondary")["game_variant_main_position_id"]
            .to_string()
            .trim_matches('"')],
        fact(&c, "position.secondary")["position"]
    );

    let elements = fact(&c, "element.ids");
    let map = &elements["map"];
    assert_eq!(
        map["2"],
        fact(&c, "element.primary")["element"],
        "the character element is id 2"
    );
    for e in elements["evidence"].as_array().unwrap() {
        let skill = e["skill"].as_str().unwrap();
        let tech = fact(&c, &format!("technique.{skill}"));
        let id = tech["element_id"].as_u64().unwrap();
        assert_eq!(
            e["game_element_id"], id,
            "{skill}: the evidence and the technique fact disagree on the element id"
        );
        match e["wiki_element"].as_str() {
            Some(wiki) => assert_eq!(
                map[id.to_string()],
                wiki,
                "{skill}: the map disagrees with the wiki element"
            ),
            None => assert_eq!(
                map[id.to_string()],
                "none",
                "{skill}: a skill with no wiki element must map to none"
            ),
        }
    }
}

#[test]
fn the_codex_the_native_export_and_the_hero_stats_are_rows_of_one_growth_table() {
    let (c, _) = report();
    let t = fact(&c, "stat.growth_table");
    let order: Vec<&str> = t["order"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap())
        .collect();
    assert_eq!(
        order,
        [
            "kick",
            "control",
            "technique",
            "pressure",
            "physical",
            "agility",
            "intelligence"
        ]
    );
    let row = |lv: &str, rank: &Value| -> Vec<u64> {
        t[lv][rank.to_string().trim_matches('"')]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap())
            .collect()
    };
    let stats = |id: &str| -> Vec<u64> {
        order
            .iter()
            .map(|k| fact(&c, id)[*k].as_u64().unwrap())
            .collect()
    };

    assert_eq!(
        stats("stat.codex.lv50"),
        row("lv50", &t["codex_rank"]),
        "the codex is the table at its rank"
    );
    assert_eq!(stats("stat.game.lv50"), row("lv50", &t["native_rank"]));
    assert_eq!(stats("stat.game.lv99"), row("lv99", &t["native_rank"]));
    assert_eq!(stats("stat.game.hero_lv99"), row("lv99", &t["hero_rank"]));
    assert_eq!(
        fact(&c, "stat.game.hero_lv99")["total"].as_u64().unwrap(),
        stats("stat.game.hero_lv99").iter().sum::<u64>()
    );
    // The stats grow with the rank, so the codex row sits between the native and the hero rows.
    for ((n, k), h) in row("lv50", &t["native_rank"])
        .iter()
        .zip(row("lv50", &t["codex_rank"]))
        .zip(row("lv50", &t["hero_rank"]))
    {
        assert!(n <= &k && k <= h);
    }
}

#[test]
fn the_french_and_japanese_technique_names_that_were_unsourced_are_now_sourced() {
    let (c, _) = report();
    let names = |skill: &str| fact(&c, &format!("technique.{skill}"))["names"].clone();
    assert_eq!(names("rho10010")["fr"], "Frappe Poussiéreuse");
    assert_eq!(names("rho10010")["en_dub"], "Dust Kick");
    assert_eq!(names("rho10040")["fr"], "Petit Pont");
    assert_eq!(names("rho10040")["ja"], "また抜き");
    assert_eq!(names("whd00190")["fr"], "Pied Céleste");
    assert_eq!(names("whd00190")["ja"], "裁きの鉄槌");
    assert_eq!(names("whs00310")["ja"], "ディバインアロー");
    assert_eq!(names("whs01080")["ja"], "ヘブンドライブ");
    for f in c
        .canon
        .facts
        .iter()
        .filter(|f| f.id.starts_with("technique.") && f.value.get("skill_code").is_some())
    {
        let n = &f.value["names"];
        assert!(
            n["en"].is_string() && n["ja"].is_string() && n["fr"].is_string(),
            "{}: a technique name is still missing: {n}",
            f.id
        );
    }
}

#[test]
fn what_stays_unresolved_is_listed_in_merge_json() {
    let (c, r) = report();
    let open: Vec<&str> = c.merge["conflicts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| x["status"] == "open")
        .map(|x| x["id"].as_str().unwrap())
        .collect();
    assert_eq!(open.len(), r.conflicts_open);
    assert_eq!(open, ["c9", "c10"]);
    let unsourced = c.merge["unsourced"].as_array().unwrap();
    for needle in ["conflict c9", "conflict c10"] {
        assert!(
            unsourced
                .iter()
                .any(|u| u.as_str().unwrap().contains(needle)),
            "{needle} is open but not listed as unsourced"
        );
    }
    assert!(
        r.conflicts_resolved >= 11,
        "resolved: {}",
        r.conflicts_resolved
    );
}
