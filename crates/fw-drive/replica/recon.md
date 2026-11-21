# Recon: Google Drive (web), core file loop

Scope: the file manager of drive.google.com on the web, rebuilt in Rust as `fw-drive`. Source: the owner's own account, read 2026-11-21 in Chrome (layout and computed styles only; no bundle, no private endpoint), plus the public Drive API v3 reference for the data model. Screenshots stay out of the repository.

Core loop: put a file in, find it again, share it. Happy path to upload: New, File upload, pick (3 clicks).

## Screens

| ID | screen | route | purpose | key components | states |
| --- | --- | --- | --- | --- | --- |
| S01 | My Drive | `/drive/my-drive` | browse own files | nav rail, search bar, folder title menu, filter chips, list/grid toggle, file table | empty, loading, filled, drag-over |
| S02 | Folder | `/drive/folders/<id>` | browse a folder | breadcrumb, same table | empty, filled, permission denied |
| S03 | Home | `/drive/home` | suggested and recent | file table | empty, filled |
| S04 | Shared with me | `/drive/shared-with-me` | files others shared | table with owner column | empty, filled |
| S05 | Recent | `/drive/recent` | by last change | table grouped by date | empty, filled |
| S06 | Starred | `/drive/starred` | pinned items | table | empty, filled |
| S07 | Trash | `/drive/trash` | restore or delete forever | table, empty-trash action | empty, filled |
| S08 | Storage | `/drive/quota` | usage | largest-first table, quota bar | filled |
| S09 | Search results | `/drive/search?q=` | find files | chips: type, people, modified | no results, filled |
| S10 | Preview | overlay | view a file | toolbar: download, share, close | image, pdf, text, video, unsupported |
| S11 | Share dialog | modal | grant access | people input, role select, general access, copy link | owner, editor, viewer |
| S12 | New menu | menu | create | new folder, file upload, folder upload | open |
| S13 | Rename / move dialogs | modal | edit metadata | text field, folder picker | error: name taken is allowed, cycle refused |

Measured (Chrome computed styles): body 14 px Google Sans, text rgb(31,31,31) on white; table row 48 px, weight 500, rgb(68,71,70); search form 48 px high, radius 28 px, fill rgb(233,238,246); New button 56 px, radius 16 px; nav items 32 px in a 224 px rail; filter chips 40 px.

## Flows

- F01 Upload: S01 > S12 New > File upload > row appears with progress. Edge: same name twice (Drive keeps both), quota full, upload cancelled.
- F02 Create folder: S01 > S12 > New folder > name > S02.
- F03 Find: S09 query > filter chips > open. Edge: no results.
- F04 Organise: row menu > rename, move, star, copy. Edge: move a folder into itself.
- F05 Delete and restore: row menu > move to trash > S07 > restore or delete forever, empty trash.
- F06 Share: S11 add email with role, or "anyone with the link"; S04 for the recipient.
- F07 Download and preview: double-click > S10; download keeps the name.

## Components

Nav rail item (active, hover), New FAB, search bar with filter button, filter chip (menu), segmented list/grid toggle, data table (sortable header, row hover, selected, row overflow menu), breadcrumb, menu, dialog, text field, role select, snackbar with undo, quota bar, empty state, upload progress panel.

## Data model (from Drive API v3 `File` and `Permission`)

- Node: id, name, mimeType, parent, size, md5/content hash, owner, starred, trashed, trashedTime, createdTime, modifiedTime, viewedByMeTime. Folders are nodes with the folder type. Confidence: high (public API).
- Permission: node, type (user, anyone), email, role (owner, writer, commenter, reader). Inherited down the tree. Confidence: high.
- Quota: sum of sizes owned, limit per user. Confidence: high.

## Out of scope (skip)

Docs, Sheets and Slides editors; Gemini answers; Computers (desktop sync client); Spam; Projects; OCR and full-text search inside files; Google's storage plans and billing.

Size: M. Hard parts: inherited permissions, streaming uploads with dedupe, Range downloads for video.
