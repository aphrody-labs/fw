# Lot: gpui-kit fork sync and missing components

`aphrody/crates/gpui-base` and `gpui-component` are forks of longbridge/gpui-kit (0.10.0). Against upstream HEAD `4373234ff46df3a3227972788a41dc6603d352ce` (CRLF normalised): gpui-base has 159 identical, 37 differing and 16 ours-only files; gpui-component has 183 identical, 34 differing and 34 ours-only files (the ours-only count includes the `material3` module and files renamed from `mod.rs`).

Work:

1. Rebase the fork on upstream HEAD, keep `material3`, `compose15` and the no-`mod.rs` renames as the patch set; record the differing files and the reason for each.
2. Missing against Base UI and shadcn (matrix rows): `navigation-menu` (rust-ui has `navigation_menu.rs`), `aspect-ratio` (dioxus-components), `card` (rust-ui), `native-select`, `trace`; partial: autocomplete, checkbox-group, drawer swipe, scroll-area, meter, fieldset.
3. Missing against MUI that Material 3 defines: Card variants, assist, input and suggestion chips, SpeedDial (no M3 equivalent: record as not applicable), image list.

Exit: the fork's diff against upstream is a documented patch set; each missing component is added or marked out of scope.
