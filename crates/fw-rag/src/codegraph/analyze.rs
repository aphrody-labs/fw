// SPDX-License-Identifier: Apache-2.0
//! Graph analysis: god nodes, Louvain community detection and modularity. Deterministic: no
//! randomness, fixed iteration order, ties broken by the smaller index.
//!
//! External nodes (symbols of crates and packages outside the corpus) are context, not
//! architecture: they are never god nodes and take no part in community detection, so a
//! `serde_json::Value` used everywhere cannot glue unrelated code into one community.

use std::collections::{BTreeMap, HashMap};

use crate::codegraph::{extract::ids::make_id, graph::Graph};

/// Relations that describe how a symbol is organised, not what depends on what.
const STRUCTURAL: &[&str] = &["contains", "method", "case_of"];

impl Graph {
    /// Whether a node is the file node of its source file.
    #[must_use]
    pub fn is_file_node(&self, node: usize) -> bool {
        let n = &self.nodes[node];
        n.file
            .as_deref()
            .is_some_and(|file| n.id == make_id(&[file]))
    }

    /// The dependency links of a node and of its members (the methods, constants and variants
    /// of a type): `(edge, other node)` for calls, references, imports, implements and inherits
    /// to nodes of the corpus outside that group. Structure (`contains`, `method`, `case_of`),
    /// links to external nodes and links among the members themselves do not count.
    fn hub_links(&self, node: usize) -> Vec<(usize, usize)> {
        let mut group = vec![node];
        group.extend(
            self.out_adj[node]
                .iter()
                .filter(|&&e| STRUCTURAL.contains(&self.edges[e].relation.as_str()))
                .map(|&e| self.edges[e].dst)
                .filter(|&member| !self.is_file_node(member)),
        );
        let mut links = Vec::new();
        for &g in &group {
            for (e, other) in self.out_adj[g]
                .iter()
                .map(|&e| (e, self.edges[e].dst))
                .chain(self.in_adj[g].iter().map(|&e| (e, self.edges[e].src)))
            {
                if !group.contains(&other)
                    && !self.nodes[other].external
                    && !STRUCTURAL.contains(&self.edges[e].relation.as_str())
                {
                    links.push((e, other));
                }
            }
        }
        links
    }

    /// Dependency edges of a node and its members ([`Graph::hub_links`]): how much of the
    /// corpus leans on it.
    #[must_use]
    pub fn hub_degree(&self, node: usize) -> usize {
        self.hub_links(node).len()
    }

    /// How many distinct other files hold a dependency edge of a node and its members.
    #[must_use]
    pub fn hub_files(&self, node: usize) -> usize {
        let own = self.nodes[node].file.as_deref();
        self.hub_links(node)
            .into_iter()
            .filter_map(|(_, other)| self.nodes[other].file.as_deref())
            .filter(|file| Some(*file) != own)
            .collect::<std::collections::HashSet<_>>()
            .len()
    }

    /// The `limit` real hubs: symbols (not files, not external, not generated, not a crate's
    /// `Result` alias) with the most dependency edges ([`Graph::hub_degree`]), then id, highest
    /// first.
    #[must_use]
    pub fn god_nodes(&self, limit: usize) -> Vec<(usize, usize)> {
        let mut ranked: Vec<(usize, usize)> = (0..self.nodes.len())
            .filter(|&i| {
                let n = &self.nodes[i];
                // `Result` is the error alias every crate defines: a hub by convention.
                !n.external && !n.generated && n.label != "Result" && !self.is_file_node(i)
            })
            .map(|i| (i, self.hub_degree(i)))
            .filter(|&(_, degree)| degree > 0)
            .collect();
        ranked.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| self.nodes[a.0].id.cmp(&self.nodes[b.0].id))
        });
        ranked.truncate(limit);
        ranked
    }

    /// Undirected weighted adjacency with parallel edges summed, self loops and links to
    /// external nodes dropped.
    fn undirected(&self) -> Vec<Vec<(usize, f64)>> {
        let mut maps: Vec<BTreeMap<usize, f64>> = vec![BTreeMap::new(); self.nodes.len()];
        for edge in &self.edges {
            if edge.src == edge.dst
                || self.nodes[edge.src].external
                || self.nodes[edge.dst].external
            {
                continue;
            }
            let w = if edge.weight > 0.0 { edge.weight } else { 1.0 };
            *maps[edge.src].entry(edge.dst).or_insert(0.0) += w;
            *maps[edge.dst].entry(edge.src).or_insert(0.0) += w;
        }
        maps.into_iter().map(|m| m.into_iter().collect()).collect()
    }

    /// Newman modularity of a community assignment (`None` entries are singletons).
    #[must_use]
    pub fn modularity(&self, community: &[Option<i64>]) -> f64 {
        let adj = self.undirected();
        let two_m: f64 = adj.iter().flat_map(|a| a.iter().map(|(_, w)| w)).sum();
        if two_m == 0.0 {
            return 0.0;
        }
        let label = |i: usize| {
            community
                .get(i)
                .copied()
                .flatten()
                .unwrap_or(-(i as i64) - 1)
        };
        let mut inside: HashMap<i64, f64> = HashMap::new();
        let mut total: HashMap<i64, f64> = HashMap::new();
        for (i, neighbors) in adj.iter().enumerate() {
            let ci = label(i);
            for &(j, w) in neighbors {
                *total.entry(ci).or_insert(0.0) += w;
                if label(j) == ci {
                    *inside.entry(ci).or_insert(0.0) += w;
                }
            }
        }
        total
            .iter()
            .map(|(c, tot)| inside.get(c).copied().unwrap_or(0.0) / two_m - (tot / two_m).powi(2))
            .sum()
    }

    /// Detect communities with Louvain and write them into the nodes: ids are ranked by size
    /// (0 is the largest), the name is the label of the best-connected member. Returns the
    /// modularity reached.
    pub fn detect_communities(&mut self) -> f64 {
        let adj = self.undirected();
        let n = self.nodes.len();
        let mut membership: Vec<usize> = (0..n).collect();
        let mut level_adj = adj;
        let mut level_to_original: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
        loop {
            let (assign, improved) = louvain_pass(&level_adj);
            if !improved {
                break;
            }
            let communities = assign.iter().copied().max().map_or(0, |m| m + 1);
            for (level_node, &c) in assign.iter().enumerate() {
                for &orig in &level_to_original[level_node] {
                    membership[orig] = c;
                }
            }
            let mut next_members: Vec<Vec<usize>> = vec![Vec::new(); communities];
            for (level_node, &c) in assign.iter().enumerate() {
                next_members[c].extend(level_to_original[level_node].iter().copied());
            }
            let mut merged: Vec<BTreeMap<usize, f64>> = vec![BTreeMap::new(); communities];
            for (u, neighbors) in level_adj.iter().enumerate() {
                for &(v, w) in neighbors {
                    let (cu, cv) = (assign[u], assign[v]);
                    *merged[cu].entry(cv).or_insert(0.0) += w;
                }
            }
            level_adj = merged
                .into_iter()
                .map(|m| m.into_iter().collect())
                .collect();
            level_to_original = next_members;
            if communities == 1 {
                break;
            }
        }
        // Rank communities by size (desc) then by smallest member, isolated nodes included;
        // external nodes belong to no community.
        let mut members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (node, &c) in membership.iter().enumerate() {
            if self.nodes[node].external {
                self.nodes[node].community = None;
                self.nodes[node].community_name = None;
                continue;
            }
            members.entry(c).or_default().push(node);
        }
        let mut ranked: Vec<Vec<usize>> = members.into_values().collect();
        ranked.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));
        let mut assignment: Vec<Option<i64>> = vec![None; n];
        for (rank, group) in ranked.iter().enumerate() {
            // Named after its biggest hub, a symbol rather than a file when it has one.
            let best = group
                .iter()
                .copied()
                .max_by(|&x, &y| {
                    (!self.is_file_node(x))
                        .cmp(&!self.is_file_node(y))
                        .then_with(|| self.hub_degree(x).cmp(&self.hub_degree(y)))
                        .then_with(|| self.degree(x).cmp(&self.degree(y)))
                        .then_with(|| self.nodes[y].id.cmp(&self.nodes[x].id))
                })
                .unwrap_or(group[0]);
            let name = self.nodes[best].label.clone();
            for &node in group {
                assignment[node] = Some(rank as i64);
                self.nodes[node].community = Some(rank as i64);
                self.nodes[node].community_name = Some(name.clone());
            }
        }
        self.modularity(&assignment)
    }
}

/// One Louvain local-moving pass over `adj`; returns the compacted community of every node and
/// whether any node moved.
fn louvain_pass(adj: &[Vec<(usize, f64)>]) -> (Vec<usize>, bool) {
    let n = adj.len();
    let degree: Vec<f64> = adj.iter().map(|a| a.iter().map(|(_, w)| w).sum()).collect();
    let two_m: f64 = degree.iter().sum();
    let mut community: Vec<usize> = (0..n).collect();
    if two_m == 0.0 {
        return (community, false);
    }
    let mut total: Vec<f64> = degree.clone();
    let mut improved = false;
    for _ in 0..50 {
        let mut moved = false;
        for node in 0..n {
            let current = community[node];
            let mut links: BTreeMap<usize, f64> = BTreeMap::new();
            for &(other, w) in &adj[node] {
                if other != node {
                    *links.entry(community[other]).or_insert(0.0) += w;
                }
            }
            total[current] -= degree[node];
            let stay =
                links.get(&current).copied().unwrap_or(0.0) - total[current] * degree[node] / two_m;
            let mut best = (current, stay);
            for (&candidate, &weight) in &links {
                let gain = weight - total[candidate] * degree[node] / two_m;
                if gain > best.1 + 1e-12
                    || ((gain - best.1).abs() <= 1e-12
                        && candidate < best.0
                        && candidate != current
                        && gain > stay + 1e-12)
                {
                    best = (candidate, gain);
                }
            }
            total[best.0] += degree[node];
            if best.0 != current {
                community[node] = best.0;
                moved = true;
                improved = true;
            }
        }
        if !moved {
            break;
        }
    }
    let mut remap: BTreeMap<usize, usize> = BTreeMap::new();
    for c in &community {
        let next = remap.len();
        remap.entry(*c).or_insert(next);
    }
    (community.iter().map(|c| remap[c]).collect(), improved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegraph::graph::{Edge, Node};

    /// Two triangles joined by a single bridge: two communities are the obvious answer.
    fn two_triangles() -> Graph {
        let nodes: Vec<Node> = (0..6)
            .map(|i| Node::code(&format!("n{i}"), &format!("N{i}"), None))
            .collect();
        let pairs = [(0, 1), (1, 2), (0, 2), (3, 4), (4, 5), (3, 5), (2, 3)];
        Graph::from_parts(
            nodes,
            pairs
                .iter()
                .map(|&(a, b)| Edge::extracted(a, b, "calls"))
                .collect(),
        )
    }

    #[test]
    fn louvain_splits_two_triangles_and_scores_positive_modularity() {
        let mut g = two_triangles();
        let q = g.detect_communities();
        assert!(q > 0.3, "modularity {q}");
        let c = |i: usize| g.nodes[i].community.unwrap();
        assert_eq!(c(0), c(1));
        assert_eq!(c(1), c(2));
        assert_eq!(c(3), c(4));
        assert_eq!(c(4), c(5));
        assert_ne!(c(0), c(3));
        assert!(g.nodes[0].community_name.is_some());
    }

    #[test]
    fn detection_is_deterministic() {
        let mut a = two_triangles();
        let mut b = two_triangles();
        a.detect_communities();
        b.detect_communities();
        let ids = |g: &Graph| g.nodes.iter().map(|n| n.community).collect::<Vec<_>>();
        assert_eq!(ids(&a), ids(&b));
    }

    #[test]
    fn isolated_nodes_become_their_own_communities_and_empty_graphs_are_fine() {
        let mut g = crate::codegraph::graph::sample();
        g.detect_communities();
        assert!(g.nodes[4].community.is_some());
        let mut empty = Graph::default();
        assert_eq!(empty.detect_communities(), 0.0);
    }

    #[test]
    fn god_nodes_rank_by_degree() {
        let g = crate::codegraph::graph::sample();
        let top = g.god_nodes(2);
        // `.upsert_doc()` is called by `.open()` and calls `Connection`; `contains` is structure.
        assert_eq!(top[0], (2, 2));
        assert_eq!(top.len(), 2);
        // `RagStore` owns both methods: it carries what leans on them (`.upsert_doc()` calls
        // `Connection`), not the call between its own methods.
        assert_eq!(g.hub_degree(0), 1);
    }

    #[test]
    fn modularity_of_a_trivial_partition_is_not_positive() {
        let g = two_triangles();
        let one = vec![Some(0); 6];
        assert!(g.modularity(&one).abs() < 1e-9);
    }

    #[test]
    fn external_nodes_are_never_hubs_and_never_glue_communities_together() {
        let mut g = two_triangles();
        // One external symbol used by every node: it would merge both triangles and top the hubs.
        let nodes: Vec<Node> = g
            .nodes
            .iter()
            .cloned()
            .chain([{
                let mut n = Node::code("ext", "serde_json::Value", None);
                n.external = true;
                n
            }])
            .collect();
        let mut edges = g.edges.clone();
        edges.extend((0..6).map(|i| Edge::extracted(i, 6, "references")));
        g = Graph::from_parts(nodes, edges);
        let q = g.detect_communities();
        assert!(q > 0.3, "modularity {q}");
        let c = |i: usize| g.nodes[i].community;
        assert_ne!(c(0), c(3), "the external symbol does not merge the triangles");
        assert_eq!((c(6), g.nodes[6].community_name.clone()), (None, None));
        let ranks: std::collections::HashSet<_> = g.nodes[..6].iter().filter_map(|n| n.community).collect();
        assert_eq!(ranks.len(), 2, "no community was spent on the external node");
        assert!(g.god_nodes(10).iter().all(|&(i, _)| i != 6));
        assert_eq!(g.hub_degree(0), 2, "links to external nodes do not count");
    }

    #[test]
    fn files_generated_nodes_and_structure_do_not_make_hubs() {
        let mut nodes = vec![
            Node::code("src_a_rs", "a.rs", Some("src/a.rs")),
            Node::code("src_a_f", "f()", Some("src/a.rs")),
            Node::code("src_a_t", "T", Some("src/a.rs")),
            Node::code("g", "Generated", Some("gen.rs")),
        ];
        nodes[3].generated = true;
        let edges = vec![
            Edge::extracted(0, 1, "contains"),
            Edge::extracted(0, 2, "contains"),
            Edge::extracted(1, 2, "references"),
            Edge::extracted(3, 2, "references"),
            Edge::extracted(1, 3, "calls"),
        ];
        let g = Graph::from_parts(nodes, edges);
        let names: Vec<&str> = g.god_nodes(10).iter().map(|&(i, _)| g.nodes[i].label.as_str()).collect();
        assert_eq!(names, ["f()", "T"], "two dependency edges each, ties by id");
    }
}
