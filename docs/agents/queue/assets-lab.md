# assets-lab: Byron Love 3D, M3 corpus, Bevy study (handoff)

Goal: private repo `aphrody-labs/assets-lab` (clone `C:\src\assets-lab`): complete rigged and animated Byron Love GLB from the 8 LEVEL5 renders (private tests only), M3 test corpus, Bevy study and skill, 3D sprite sheets for `@aphrody/pets`.

## Done (verified on origin)
- assets-lab `e655688` scaffold, `4fd06cd` M3 corpus (373 fixtures, 21,494,550 B, manifest `corpus/m3/manifest.json`, glyph check, docs/m3-corpus.md), `5e3e4e7` Byron pipeline.
- aphrody `a381925c20`: snapped glyph SVGs (aphrody_character, _fill).
- nie `bfb77826e`: skill `bevy` (`.claude/skills/bevy`, SKILL.md + references).
- fw `8489e425`: sources.toml entry absorbing the bevy skill.
- Byron steps run and viewed: Hunyuan3D-2mv shape (157,496 v / 314,988 f, 5,670,596 B raw GLB, 124 s, 5,834 MB peak VRAM, seed 12345), cleanup to 30,000 f (18,278 v, 145 charts), 8-view projected albedo 2048 (IoU 0.71-0.87), Cycles normal and AO bake, ORM pack (albedo 3,657,960 B, normal 3,609,681 B, orm 2,143,622 B PNG), landmark rig (21 bones, auto weights), 7 actions idle/run/wave/jump/fail/wait/review. `assets/blend/byron_anim.blend` holds rig + actions; contact sheets in `docs/img`.

## Remaining
1. `blender/export.py` (not written): M3 stage (tokens via Bun TOML to JSON: surface, containers, primary/tertiary), materials from `assets/tex`, export GLB (`export_animation_mode='ACTIONS'`): `byron.glb` and `byron_stage.glb`.
2. `scripts/export.ts` (gltf-transform): swap in exact `assets/tex/{albedo,normal,orm}.png`, occlusion = ORM, prune/dedup/resample, KTX2 via `toktx` (KHR_texture_basisu: ETC1S albedo, UASTC normal), meshopt and Draco variants; report bytes before/after, triangles, texture bytes. Bevy 0.20.1 support per nie skill `references/gltf.md`: export a Bevy-loadable variant accordingly.
3. `scripts/validate.ts` with `gltf-validator` (installed).
4. Renders: Bevy (build on fr1 under `/srv/xbuild/assets-lab`, never `~`; fr1 has no GPU, use lavapipe) and three.js WebGPU (Browser pane screenshot).
5. Frames: Blender workbench/eevee ortho, 8 yaw directions per animation frame (state frame counts: idle 6, running-right 8, running-left 8, waving 4, jumping 5, failed 8, waiting 6, running 6, review 6), then `pets/byron-love-3d.json` plan in assets-lab (frames are LEVEL5: do not put them in the public openpets repo), build with `bun C:/src/openpets/packages/aphrody/src/cli.ts build <plan> --out <dir>`, compare with 153,044 B sheet.
6. `RESULTS.md` (numbers, screenshots), README already states private/IP.
7. UniRig not attempted (needs flash-attn/spconv builds on Windows); rig is landmark-based. Figma kit not read (MCP needs authorization).

## Resume
```
cd C:\src\assets-lab && bun install
uv venv --python 3.13 py/.venv && uv pip install --python py/.venv/Scripts/python.exe torch torchvision --index-url https://download.pytorch.org/whl/cu128 && uv pip install --python py/.venv/Scripts/python.exe -r py/requirements.txt
git clone https://github.com/Tencent-Hunyuan/Hunyuan3D-2 work/Hunyuan3D-2 && git -C work/Hunyuan3D-2 checkout f8db63096c8282cb27354314d896feba5ba6ff8a
bun scripts/pipeline.ts          # or one step: fetch generate clean texture bake pack rig animate
```
Steps were run individually; `scripts/pipeline.ts` only type-bundled, not run end to end. Needs `transformers<5` (v5 renames the DINOv2 keys).

## Directories created
- `C:\src\assets-lab` (committed; `py\.venv` 5.1 GB and `work\` 195 MB are untracked and regenerable: safe to delete).
- `C:\Users\aphro\.cache\huggingface\hub\models--tencent--Hunyuan3D-2mv` 9.2 GB (ckpt and safetensors both fetched, only fp16 safetensors used): safe to delete, 14 min to re-download.
- fr1, prd: none created by this task.
