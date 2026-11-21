# Lot: motion.dev features missing from gpui-base motion

Present in `aphrody/crates/gpui-base/src/motion*` (spring, keyframes, easing, presence, stagger, sequence, timing, reveal, reduce motion, scroll bounce) and `material3/compose15.rs` (`M3MotionScheme`). Missing, each confirmed by a grep with 0 hits over gpui-base, gpui-component, gpui_ui and aphrody_ui: layout animation (FLIP and shared layout), drag with constraints and pan sessions, general inertia, scroll-linked progress and in-view observation, Reorder, MotionValue transforms (`useTransform`).

Work in order of user impact: layout animation, gestures (drag, pan, inertia), scroll-linked values, Reorder. Land upstream in longbridge/gpui-kit when it fits their base crate, else keep in the patch set of the sync lot.
