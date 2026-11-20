use std::{env, fs, path::PathBuf};

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    generate_design_tokens(&out_dir);
}

fn generate_design_tokens(out_dir: &std::path::Path) {
    use std::collections::BTreeMap;
    use std::fmt::Write as _;

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let tokens_path = PathBuf::from(&manifest_dir).join("tokens/google_design_tokens.toml");
    println!("cargo:rerun-if-changed={}", tokens_path.display());

    let raw = fs::read_to_string(&tokens_path).expect("failed to read tokens/google_design_tokens.toml");
    let root: toml::Value = toml::from_str(&raw).expect("failed to parse google_design_tokens.toml");

    fn req_table<'a>(
        val: &'a toml::Value,
        key: &str,
    ) -> &'a toml::map::Map<String, toml::Value> {
        val.get(key)
            .and_then(|v| v.as_table())
            .unwrap_or_else(|| panic!("google_design_tokens.toml missing table `{key}`"))
    }
    fn req_str<'a>(tbl: &'a toml::map::Map<String, toml::Value>, key: &str) -> &'a str {
        tbl.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("google_design_tokens.toml missing string `{key}`"))
    }
    fn req_i64(tbl: &toml::map::Map<String, toml::Value>, key: &str) -> i64 {
        tbl.get(key)
            .and_then(|v| v.as_integer())
            .unwrap_or_else(|| panic!("google_design_tokens.toml missing integer `{key}`"))
    }
    let table = req_table;
    let fmt_f32 = |tbl: &toml::map::Map<String, toml::Value>, key: &str| -> String {
        match tbl.get(key) {
            Some(toml::Value::Float(f)) => {
                let s = f.to_string();
                if s.contains('.') {
                    format!("{s}_f32")
                } else {
                    format!("{s}.0_f32")
                }
            }
            Some(toml::Value::Integer(i)) => format!("{i}.0_f32"),
            other => panic!("google_design_tokens.toml expected number `{key}`, got {other:?}"),
        }
    };
    let hex_to_rgb_literal = |hex: &str| -> String {
        let clean = hex
            .trim()
            .strip_prefix('#')
            .unwrap_or_else(|| panic!("invalid hex color `{hex}` in google_design_tokens.toml"));
        assert!(
            clean.len() >= 6 && clean[..6].chars().all(|c| c.is_ascii_hexdigit()),
            "invalid hex color `{hex}`"
        );
        format!("0x{}_u32", clean[..6].to_ascii_uppercase())
    };
    let rust_str = |s: &str| -> String { format!("{s:?}") };

    let root_tbl = root
        .as_table()
        .expect("google_design_tokens.toml root must be a table");
    let schema_version = req_i64(root_tbl, "schema_version") as u32;
    let source_of_truth = req_str(root_tbl, "source_of_truth");
    let system_name = req_str(root_tbl, "system");
    let spec_url = req_str(root_tbl, "spec_url");
    let fonts_url = req_str(root_tbl, "fonts_url");
    let stitch_url = req_str(root_tbl, "stitch_url");
    let sans_flex_family = req_str(root_tbl, "sans_flex_family");
    let synced_at = req_str(root_tbl, "synced_at");
    let seed_hex = req_str(root_tbl, "seed_hex");
    let variant = req_str(root_tbl, "variant");
    let principles: Vec<String> = root_tbl
        .get("principles")
        .and_then(|v| v.as_array())
        .expect("missing principles array")
        .iter()
        .map(|v| rust_str(v.as_str().expect("principle entry must be string")))
        .collect();

    let ref_tbl = table(&root, "ref");
    let typeface_tbl = ref_tbl
        .get("typeface")
        .and_then(|v| v.as_table())
        .expect("missing [ref.typeface]");
    let ref_palette_tbl = ref_tbl
        .get("palette")
        .and_then(|v| v.as_table())
        .expect("missing [ref.palette]");
    let mut ref_palette_sorted: BTreeMap<&str, &str> = BTreeMap::new();
    for (k, v) in ref_palette_tbl {
        ref_palette_sorted.insert(k.as_str(), v.as_str().expect("ref.palette value must be string"));
    }

    let axes_tbl = ref_tbl
        .get("variable_axes")
        .and_then(|v| v.as_table())
        .expect("missing [ref.variable_axes]");
    let sys_tbl = table(&root, "sys");
    let shape_tbl = sys_tbl
        .get("shape")
        .and_then(|v| v.as_table())
        .expect("missing [sys.shape]");
    let state_tbl = sys_tbl
        .get("state")
        .and_then(|v| v.as_table())
        .expect("missing [sys.state]");
    let density_tbl = sys_tbl
        .get("density")
        .and_then(|v| v.as_table())
        .expect("missing [sys.density]");
    let window_tbl = sys_tbl
        .get("window_size_classes")
        .and_then(|v| v.as_table())
        .expect("missing [sys.window_size_classes]");
    let motion_tbl = sys_tbl
        .get("motion")
        .and_then(|v| v.as_table())
        .expect("missing [sys.motion]");
    let springs_tbl = motion_tbl
        .get("springs")
        .and_then(|v| v.as_table())
        .expect("missing [sys.motion.springs]");
    let typescale_tbl = sys_tbl
        .get("typescale")
        .and_then(|v| v.as_table())
        .expect("missing [sys.typescale]");
    let color_tbl = sys_tbl
        .get("color")
        .and_then(|v| v.as_table())
        .expect("missing [sys.color]");

    let mut out = String::with_capacity(16_384);
    let _ = writeln!(out, "pub const SCHEMA_VERSION: u32 = {schema_version};");
    let _ = writeln!(out, "pub const SOURCE_OF_TRUTH: &str = {};", rust_str(source_of_truth));
    let _ = writeln!(out, "pub const SYSTEM_NAME: &str = {};", rust_str(system_name));
    let _ = writeln!(out, "pub const SPEC_URL: &str = {};", rust_str(spec_url));
    let _ = writeln!(out, "pub const FONTS_URL: &str = {};", rust_str(fonts_url));
    let _ = writeln!(out, "pub const STITCH_URL: &str = {};", rust_str(stitch_url));
    let _ = writeln!(out, "pub const SANS_FLEX_FAMILY: &str = {};", rust_str(sans_flex_family));
    let _ = writeln!(out, "pub const SYNCED_AT: &str = {};", rust_str(synced_at));
    let _ = writeln!(out, "pub const SEED_HEX: &str = {};", rust_str(seed_hex));
    let _ = writeln!(out, "pub const VARIANT: &str = {};", rust_str(variant));
    let _ = writeln!(out, "pub const PRINCIPLES: &[&str] = &[{}];", principles.join(", "));
    let _ = writeln!(out, "pub const TYPEFACE_BRAND: &str = {};", rust_str(req_str(typeface_tbl, "brand")));
    let _ = writeln!(out, "pub const TYPEFACE_PLAIN: &str = {};", rust_str(req_str(typeface_tbl, "plain")));
    let _ = writeln!(out, "pub const TYPEFACE_MONO: &str = {};", rust_str(req_str(typeface_tbl, "mono")));
    let _ = writeln!(out, "pub const TYPEFACE_FALLBACK: &str = {};", rust_str(req_str(typeface_tbl, "fallback")));

    let _ = writeln!(out, "pub const REFERENCE_PALETTE: &[(&str, &str, u32)] = &[");
    for (k, hex) in &ref_palette_sorted {
        let _ = writeln!(out, "    ({}, {}, {}),", rust_str(k), rust_str(hex), hex_to_rgb_literal(hex));
    }
    let _ = writeln!(out, "];");

    let axis_order = ["opsz", "wght", "wdth", "GRAD", "slnt", "ROND"];
    let _ = writeln!(out, "pub const VARIABLE_AXES: &[AxisSpec; 6] = &[");
    for key in axis_order {
        let ax = axes_tbl
            .get(key)
            .and_then(|v| v.as_table())
            .unwrap_or_else(|| panic!("missing variable axis `{key}`"));
        let _ = writeln!(
            out,
            "    AxisSpec {{ tag: {}, name: {}, min: {}, default_value: {}, max: {} }},",
            rust_str(req_str(ax, "tag")),
            rust_str(req_str(ax, "name")),
            fmt_f32(ax, "min"),
            fmt_f32(ax, "default_value"),
            fmt_f32(ax, "max"),
        );
    }
    let _ = writeln!(out, "];");

    let _ = writeln!(
        out,
        "pub const SHAPES: ShapeScale = ShapeScale {{ none_px: {}, extra_small_px: {}, small_px: {}, medium_px: {}, large_px: {}, large_increased_px: {}, extra_large_px: {}, full_px: {} }};",
        fmt_f32(shape_tbl, "none_px"),
        fmt_f32(shape_tbl, "extra_small_px"),
        fmt_f32(shape_tbl, "small_px"),
        fmt_f32(shape_tbl, "medium_px"),
        fmt_f32(shape_tbl, "large_px"),
        fmt_f32(shape_tbl, "large_increased_px"),
        fmt_f32(shape_tbl, "extra_large_px"),
        fmt_f32(shape_tbl, "full_px"),
    );

    let _ = writeln!(
        out,
        "pub const STATES: StateLayers = StateLayers {{ hover_opacity: {}, focus_opacity: {}, pressed_opacity: {}, dragged_opacity: {}, disabled_container_opacity: {}, disabled_content_opacity: {}, focus_ring_px: {} }};",
        fmt_f32(state_tbl, "hover_opacity"),
        fmt_f32(state_tbl, "focus_opacity"),
        fmt_f32(state_tbl, "pressed_opacity"),
        fmt_f32(state_tbl, "dragged_opacity"),
        fmt_f32(state_tbl, "disabled_container_opacity"),
        fmt_f32(state_tbl, "disabled_content_opacity"),
        fmt_f32(state_tbl, "focus_ring_px"),
    );

    let _ = writeln!(
        out,
        "pub const DENSITY: DensityScale = DensityScale {{ default: {}_i32, compact_1: {}_i32, compact_2: {}_i32, compact_3: {}_i32, touch_target_min_px: {}, pointer_target_min_px: {}, control_height_min_px: {} }};",
        req_i64(density_tbl, "default"),
        req_i64(density_tbl, "compact_1"),
        req_i64(density_tbl, "compact_2"),
        req_i64(density_tbl, "compact_3"),
        fmt_f32(density_tbl, "touch_target_min_px"),
        fmt_f32(density_tbl, "pointer_target_min_px"),
        fmt_f32(density_tbl, "control_height_min_px"),
    );

    let _ = writeln!(
        out,
        "pub const WINDOW_SIZE_CLASSES: WindowSizeThresholds = WindowSizeThresholds {{ compact_max_px: {}, medium_min_px: {}, expanded_min_px: {}, large_min_px: {}, extra_large_min_px: {} }};",
        fmt_f32(window_tbl, "compact_max_px"),
        fmt_f32(window_tbl, "medium_min_px"),
        fmt_f32(window_tbl, "expanded_min_px"),
        fmt_f32(window_tbl, "large_min_px"),
        fmt_f32(window_tbl, "extra_large_min_px"),
    );

    let spring_order = [
        "spatial_fast",
        "spatial_default",
        "spatial_slow",
        "effects_fast",
        "effects_default",
        "effects_slow",
        "quire_unfold",
        "quire_settle",
        "quire_expand",
        "quire_fold",
        "quire_tick",
    ];
    let _ = writeln!(out, "pub const SPRINGS: &[SpringTokenSpec; 11] = &[");
    for key in spring_order {
        let sp = springs_tbl
            .get(key)
            .and_then(|v| v.as_table())
            .unwrap_or_else(|| panic!("missing spring `{key}`"));
        let _ = writeln!(
            out,
            "    SpringTokenSpec {{ key: {}, stiffness: {}, damping_ratio: {} }},",
            rust_str(key),
            fmt_f32(sp, "stiffness"),
            fmt_f32(sp, "damping_ratio"),
        );
    }
    let _ = writeln!(out, "];");

    let typescale_order = [
        "display_large",
        "display_medium",
        "display_small",
        "headline_large",
        "headline_medium",
        "headline_small",
        "title_large",
        "title_medium",
        "title_small",
        "body_large",
        "body_medium",
        "body_small",
        "label_large",
        "label_medium",
        "label_small",
    ];
    let fmt_ts = |key: &str| -> String {
        let ts = typescale_tbl
            .get(key)
            .and_then(|v| v.as_table())
            .unwrap_or_else(|| panic!("missing typescale `{key}`"));
        format!(
            "TypeScaleEntry {{ key: {}, font: {}, size_sp: {}, line_height_sp: {}, weight: {}, tracking_sp: {} }}",
            rust_str(key),
            rust_str(req_str(ts, "font")),
            fmt_f32(ts, "size_sp"),
            fmt_f32(ts, "line_height_sp"),
            fmt_f32(ts, "weight"),
            fmt_f32(ts, "tracking_sp"),
        )
    };
    let _ = writeln!(out, "pub const TYPESCALES: &[TypeScaleEntry; 15] = &[");
    for key in typescale_order {
        let _ = writeln!(out, "    {},", fmt_ts(key));
    }
    let _ = writeln!(out, "];");
    let _ = writeln!(out, "pub const TYPESCALE: TypeScaleSet = TypeScaleSet {{");
    for key in typescale_order {
        let _ = writeln!(out, "    {key}: {},", fmt_ts(key));
    }
    let _ = writeln!(out, "}};");

    let role_order = [
        "background",
        "on_background",
        "surface",
        "surface_dim",
        "surface_bright",
        "surface_container_lowest",
        "surface_container_low",
        "surface_container",
        "surface_container_high",
        "surface_container_highest",
        "on_surface",
        "on_surface_variant",
        "outline",
        "outline_variant",
        "primary",
        "on_primary",
        "primary_container",
        "on_primary_container",
        "secondary",
        "on_secondary",
        "secondary_container",
        "on_secondary_container",
        "tertiary",
        "on_tertiary",
        "tertiary_container",
        "on_tertiary_container",
        "error",
        "on_error",
        "error_container",
        "on_error_container",
    ];
    for (mode_key, const_suffix) in [
        ("light", "LIGHT"),
        ("dark", "DARK"),
        ("neural_obsidian", "NEURAL_OBSIDIAN"),
        ("oled_pitch", "OLED_PITCH"),
    ] {
        let mode_tbl = color_tbl
            .get(mode_key)
            .and_then(|v| v.as_table())
            .unwrap_or_else(|| panic!("missing [sys.color.{mode_key}]"));
        let _ = writeln!(out, "pub const PALETTE_{const_suffix}_HEX: ColorPaletteHex = ColorPaletteHex {{");
        for role in role_order {
            let _ = writeln!(out, "    {role}: {},", rust_str(req_str(mode_tbl, role)));
        }
        let _ = writeln!(out, "}};");
        let _ = writeln!(out, "pub const PALETTE_{const_suffix}_RGB: ColorPaletteRgb = ColorPaletteRgb {{");
        for role in role_order {
            let _ = writeln!(out, "    {role}: {},", hex_to_rgb_literal(req_str(mode_tbl, role)));
        }
        let _ = writeln!(out, "}};");
    }

    let comp_tbl = table(&root, "comp");
    let mut comp_entries = Vec::new();
    for (comp_name, comp_val) in comp_tbl {
        if let Some(fields) = comp_val.as_table() {
            for (field_name, field_val) in fields {
                let num_lit = match field_val {
                    toml::Value::Float(f) => {
                        let s = f.to_string();
                        if s.contains('.') { format!("{s}_f32") } else { format!("{s}.0_f32") }
                    }
                    toml::Value::Integer(i) => format!("{i}.0_f32"),
                    _ => continue,
                };
                comp_entries.push((comp_name.clone(), field_name.clone(), num_lit));
            }
        }
    }
    comp_entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let _ = writeln!(out, "pub const COMP_NUMBERS: &[(&str, &str, f32)] = &[");
    for (c, k, num_lit) in comp_entries {
        let _ = writeln!(out, "    ({}, {}, {num_lit}),", rust_str(&c), rust_str(&k));
    }
    let _ = writeln!(out, "];");

    fs::write(out_dir.join("design_tokens_constants.rs"), out)
        .expect("failed to write design_tokens_constants.rs");
}
