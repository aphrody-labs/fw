// SPDX-License-Identifier: Apache-2.0
//! Markdown report of a graph (summary, provenance, confidence split, relations, languages,
//! per-crate summaries and their dependencies, external dependencies, top files, god nodes,
//! communities).

use std::{
    collections::{BTreeMap, HashMap},
    fmt::Write as _,
};

use crate::codegraph::graph::{Confidence, Graph};

impl Graph {
    /// Render `GRAPH_REPORT.md` for this graph.
    #[must_use]
    pub fn report(&self, name: &str, top: usize) -> String {
        let total = self.edges.len().max(1);
        let mut confidence: BTreeMap<&str, usize> = BTreeMap::new();
        let mut relations: BTreeMap<&str, usize> = BTreeMap::new();
        let mut inferred_score = (0.0_f64, 0_usize);
        for edge in &self.edges {
            *confidence.entry(edge.confidence.as_str()).or_insert(0) += 1;
            *relations.entry(edge.relation.as_str()).or_insert(0) += 1;
            if edge.confidence == Confidence::Inferred {
                inferred_score.0 += edge.score;
                inferred_score.1 += 1;
            }
        }
        let mut sizes: BTreeMap<i64, (usize, Option<&str>)> = BTreeMap::new();
        for node in &self.nodes {
            if let Some(c) = node.community {
                let entry = sizes.entry(c).or_insert((0, None));
                entry.0 += 1;
                entry.1 = entry.1.or(node.community_name.as_deref());
            }
        }
        let mut out = String::new();
        let _ = writeln!(out, "# Graph Report - {name}\n");
        let _ = writeln!(out, "## Summary");
        if let Some(root) = &self.meta.root {
            let _ = writeln!(out, "- Root: {root}");
        }
        let _ = writeln!(
            out,
            "- Built at commit: {}",
            self.meta
                .built_at_commit
                .as_deref()
                .unwrap_or("unknown (not a Git checkout)")
        );
        let _ = writeln!(
            out,
            "- {} nodes, {} edges, {} communities",
            self.nodes.len(),
            self.edges.len(),
            sizes.len()
        );
        let external = self.nodes.iter().filter(|n| n.external).count();
        let generated = self.nodes.iter().filter(|n| n.generated).count();
        if external > 0 {
            let _ = writeln!(
                out,
                "- External symbols: {external} nodes (outside the corpus; no community, never a hub)"
            );
        }
        if let Some(skipped) = self.meta.generated_skipped.filter(|n| *n > 0) {
            let _ = writeln!(out, "- Generated files left out: {skipped}");
        }
        if generated > 0 {
            let _ = writeln!(out, "- Generated nodes included on request: {generated}");
        }
        let share = |label: &str| confidence.get(label).copied().unwrap_or(0) * 100 / total;
        let _ = writeln!(
            out,
            "- Extraction: {} % EXTRACTED, {} % INFERRED, {} % AMBIGUOUS",
            share("EXTRACTED"),
            share("INFERRED"),
            share("AMBIGUOUS")
        );
        if inferred_score.1 > 0 {
            let _ = writeln!(
                out,
                "- INFERRED average confidence: {:.2}",
                inferred_score.0 / inferred_score.1 as f64
            );
        }
        let _ = writeln!(out, "\n## Relations");
        let mut rel: Vec<_> = relations.into_iter().collect();
        rel.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        for (name, n) in rel.iter().take(top) {
            let _ = writeln!(out, "- {name}: {n}");
        }
        self.write_languages_and_packages(&mut out, top);
        self.write_externals(&mut out, top);
        let _ = writeln!(out, "\n## Top files");
        for (file, symbols, degree) in self.top_files(top) {
            let _ = writeln!(out, "- {file}: {symbols} nodes, {degree} edges");
        }
        let _ = writeln!(out, "\n## God nodes");
        for (i, degree) in self.god_nodes(top) {
            let n = &self.nodes[i];
            let _ = writeln!(
                out,
                "- {} ({degree} dependency edges, {} files){}",
                n.label,
                self.hub_files(i),
                n.file
                    .as_ref()
                    .map_or_else(String::new, |f| format!(" - {f}"))
            );
        }
        let _ = writeln!(out, "\n## Communities");
        let mut comm: Vec<_> = sizes.into_iter().collect();
        comm.sort_by(|a, b| b.1.0.cmp(&a.1.0).then_with(|| a.0.cmp(&b.0)));
        for (id, (size, name)) in comm.iter().take(top) {
            let _ = writeln!(out, "- {id}: {} ({size} nodes)", name.unwrap_or("unnamed"));
        }
        out
    }

    /// Files ranked by the edges touching their nodes: `(file, nodes, edges)`.
    fn top_files(&self, top: usize) -> Vec<(&str, usize, usize)> {
        let mut files: HashMap<&str, (usize, usize)> = HashMap::new();
        for (i, n) in self.nodes.iter().enumerate() {
            if let Some(file) = n.file.as_deref() {
                let entry = files.entry(file).or_default();
                entry.0 += 1;
                entry.1 += self.degree(i);
            }
        }
        let mut ranked: Vec<(&str, usize, usize)> =
            files.into_iter().map(|(f, (n, d))| (f, n, d)).collect();
        ranked.sort_by(|a, b| {
            b.2.cmp(&a.2)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.0.cmp(b.0))
        });
        ranked.truncate(top);
        ranked
    }

    fn write_languages_and_packages(&self, out: &mut String, top: usize) {
        let mut languages: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        let mut packages: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        let mut files_seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for n in &self.nodes {
            let Some(file) = n.file.as_deref() else {
                continue;
            };
            let new_file = usize::from(files_seen.insert(file));
            if let Some(lang) = n.lang.as_deref() {
                let entry = languages.entry(lang).or_default();
                entry.0 += new_file;
                entry.1 += 1;
            }
            if let Some(package) = n.package.as_deref() {
                let entry = packages.entry(package).or_default();
                entry.0 += new_file;
                entry.1 += 1;
            }
        }
        if !languages.is_empty() {
            let _ = writeln!(out, "\n## Languages");
            for (lang, (files, nodes)) in &languages {
                let _ = writeln!(out, "- {lang}: {files} files, {nodes} nodes");
            }
        }
        if packages.is_empty() {
            return;
        }
        let mut deps: BTreeMap<(&str, &str), usize> = BTreeMap::new();
        let mut external: BTreeMap<&str, BTreeMap<&str, usize>> = BTreeMap::new();
        for e in &self.edges {
            if !matches!(
                e.relation.as_str(),
                "calls" | "imports" | "imports_from" | "references"
            ) {
                continue;
            }
            let (src, dst) = (&self.nodes[e.src], &self.nodes[e.dst]);
            let (Some(a), Some(b)) = (src.package.as_deref(), dst.package.as_deref()) else {
                continue;
            };
            if src.external {
                continue;
            }
            if dst.external {
                *external.entry(a).or_default().entry(b).or_insert(0) += 1;
            } else if a != b {
                *deps.entry((a, b)).or_insert(0) += 1;
            }
        }
        let mut hubs: BTreeMap<&str, Vec<(usize, usize)>> = BTreeMap::new();
        for (i, n) in self.nodes.iter().enumerate() {
            if n.external || n.generated || self.is_file_node(i) {
                continue;
            }
            if let Some(package) = n.package.as_deref() {
                let degree = self.hub_degree(i);
                if degree > 0 {
                    hubs.entry(package).or_default().push((i, degree));
                }
            }
        }
        let ranked_counts = |counts: Vec<(&str, usize)>| -> String {
            let mut counts = counts;
            counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
            counts
                .iter()
                .take(3)
                .map(|(name, n)| format!("{name} ({n})"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let _ = writeln!(out, "\n## Packages ({})", packages.len());
        let mut ranked: Vec<_> = packages.into_iter().collect();
        ranked.sort_by(|a, b| b.1.1.cmp(&a.1.1).then_with(|| a.0.cmp(b.0)));
        for (package, (files, nodes)) in ranked.iter().take(top) {
            let mut line = format!("- {package}: {files} files, {nodes} nodes");
            let mut top_hubs = hubs.remove(package).unwrap_or_default();
            top_hubs.sort_by(|a, b| {
                b.1.cmp(&a.1)
                    .then_with(|| self.nodes[a.0].id.cmp(&self.nodes[b.0].id))
            });
            if !top_hubs.is_empty() {
                let names: Vec<String> = top_hubs
                    .iter()
                    .take(3)
                    .map(|&(i, d)| format!("{} ({d})", self.nodes[i].label))
                    .collect();
                let _ = write!(line, "; hubs: {}", names.join(", "));
            }
            let uses = ranked_counts(
                deps.iter()
                    .filter(|((a, _), _)| a == package)
                    .map(|((_, b), n)| (*b, *n))
                    .collect(),
            );
            if !uses.is_empty() {
                let _ = write!(line, "; uses: {uses}");
            }
            let used_by = ranked_counts(
                deps.iter()
                    .filter(|((_, b), _)| b == package)
                    .map(|((a, _), n)| (*a, *n))
                    .collect(),
            );
            if !used_by.is_empty() {
                let _ = write!(line, "; used by: {used_by}");
            }
            if let Some(counts) = external.get(package) {
                let ext = ranked_counts(counts.iter().map(|(k, v)| (*k, *v)).collect());
                let _ = write!(line, "; external: {ext}");
            }
            let _ = writeln!(out, "{line}");
        }
        if deps.is_empty() {
            return;
        }
        let _ = writeln!(out, "\n## Package dependencies ({} pairs)", deps.len());
        let mut ranked: Vec<_> = deps.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        for ((from, to), n) in ranked.iter().take(top) {
            let _ = writeln!(out, "- {from} -> {to}: {n} edges");
        }
    }

    /// The crates and packages outside the corpus the code leans on most.
    fn write_externals(&self, out: &mut String, top: usize) {
        let mut packages: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        for (i, n) in self.nodes.iter().enumerate() {
            if let (true, Some(package)) = (n.external, n.package.as_deref()) {
                let entry = packages.entry(package).or_default();
                entry.0 += 1;
                entry.1 += self.degree(i);
            }
        }
        if packages.is_empty() {
            return;
        }
        let _ = writeln!(out, "\n## External dependencies ({})", packages.len());
        let mut ranked: Vec<_> = packages.into_iter().collect();
        ranked.sort_by(|a, b| b.1.1.cmp(&a.1.1).then_with(|| a.0.cmp(b.0)));
        for (package, (items, edges)) in ranked.iter().take(top) {
            let _ = writeln!(out, "- {package}: {items} items, {edges} edges");
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::codegraph::graph::sample;

    #[test]
    fn the_report_summarises_counts_confidence_and_hubs() {
        let mut g = sample();
        g.detect_communities();
        let r = g.report("sample", 5);
        assert!(r.contains("# Graph Report - sample"));
        assert!(r.contains("5 nodes, 4 edges"));
        assert!(r.contains("75 % EXTRACTED, 25 % INFERRED"));
        assert!(r.contains("INFERRED average confidence: 0.80"));
        assert!(r.contains("- .upsert_doc() (2 dependency edges, 0 files) - store.rs"), "{r}");
        assert!(r.contains("- calls: 2"));
        assert!(r.contains("Built at commit: unknown"));
        assert!(
            r.contains("## Top files\n- store.rs: 3 nodes, 7 edges"),
            "{r}"
        );
    }

    #[test]
    fn packages_and_their_dependencies_are_listed() {
        let mut g = sample();
        for (i, package) in [(0, "rag"), (1, "rag"), (2, "rag"), (4, "other")] {
            g.nodes[i].package = Some(package.into());
        }
        g.nodes[3].package = Some("sql".into());
        g.nodes[3].file = Some("sql.rs".into());
        g.meta.built_at_commit = Some("abc123".into());
        let r = g.report("sample", 5);
        assert!(r.contains("Built at commit: abc123"));
        assert!(
            r.contains("## Packages (3)\n- rag: 1 files, 3 nodes"),
            "{r}"
        );
        assert!(r.contains("- rag -> sql: 1 edges"), "{r}");
    }

    #[test]
    fn crates_get_a_summary_and_external_dependencies_are_listed() {
        let mut g = sample();
        for (i, package) in [(0, "rag"), (1, "rag"), (2, "rag")] {
            g.nodes[i].package = Some(package.into());
        }
        g.nodes[3].file = Some("sql.rs".into());
        g.nodes[3].package = Some("sql".into());
        let mut ext = crate::codegraph::graph::Node::code("ext", "serde_json::Value", None);
        ext.external = true;
        ext.package = Some("serde_json".into());
        g.nodes.push(ext);
        g.by_id.insert("ext".into(), 5);
        g.out_adj.push(Vec::new());
        g.in_adj.push(Vec::new());
        g.edges.push(crate::codegraph::graph::Edge::extracted(1, 5, "calls"));
        g.out_adj[1].push(g.edges.len() - 1);
        g.in_adj[5].push(g.edges.len() - 1);
        g.detect_communities();
        let r = g.report("sample", 5);
        assert!(
            r.contains("- rag: 1 files, 3 nodes; hubs: .upsert_doc() (2), RagStore (1), .open() (1); uses: sql (1); external: serde_json (1)"),
            "{r}"
        );
        assert!(r.contains("- External symbols: 1 nodes"), "{r}");
        assert!(r.contains("## External dependencies (1)\n- serde_json: 1 items, 1 edges"), "{r}");
        let gods = r.split("## God nodes\n").nth(1).unwrap().split("\n\n").next().unwrap();
        assert!(!gods.contains("serde_json"), "{gods}");
        // The line the learning loop parses: `<n> nodes, <m> edges, <k> communities`.
        assert!(r.lines().any(|l| l.ends_with(" communities") && l.contains(" edges, ")));
    }
}
