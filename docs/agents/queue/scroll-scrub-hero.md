# scroll-scrub-hero: the pinned hero stage on web and GPUI (queued)

Spec: matrix row 12, R10, R12. Roadmap: M7.

## Goal
A hero whose video or sequence advances with scroll position inside a pinned stage, on the web with CSS scroll-driven animation and natively in GPUI, with a still frame under `prefers-reduced-motion`.

## Done
- Measured on the reference: section 2,136 px tall, pinned inner stage 1,068 px, one `animation-timeline: view()` rule and `view-timeline-name: --pinned-promo-timeline` in the stylesheet, 10 `position: sticky` rules.

## Remaining
1. Web: CSS-only stage (`position: sticky`, `animation-timeline: view()`) driving transform and opacity; the clip stays a normal `<video>` with a poster; browsers without scroll-driven animations show the poster.
2. GPUI: a scroll-driven element in `gpui-kit` with the same keyframes and M3 spring settle.
3. Reduced motion: a still frame, no scrubbing.

## Proof
Chrome screenshots at five scroll offsets, frame time under a 60 fps budget in a trace, the reduced-motion screenshot.
