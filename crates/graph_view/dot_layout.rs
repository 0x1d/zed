use std::collections::HashMap;

use anyhow::{anyhow, Context};
use gpui_flow::{FlowEdge, FlowNode, HandleDef, HandlePosition};
use petgraph::algo::is_cyclic_directed;
use petgraph::graph::NodeIndex;
use petgraph::prelude::*;
use petgraph::visit::EdgeRef;
use petgraph::Graph;

const NODE_GAP: f32 = 72.0;
const VERTICAL_STACK_GAP: f32 = 72.0;
const NODE_WIDTH_MIN: f32 = 128.0;
const NODE_WIDTH_MAX: f32 = 720.0;
const NODE_PAD_X: f32 = 52.0;
const LINE_TYPE_W_PER_CHAR: f32 = 7.2;
const LINE_NAME_W_PER_CHAR: f32 = 8.8;
const TWO_LINE_HEIGHT: f32 = 72.0;
const SINGLE_LINE_HEIGHT: f32 = 52.0;

/// Split display label into resource type (above) and resource name (below, bold in UI).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerraformLabelParts {
    TwoLines {
        resource_type: String,
        resource_name: String,
    },
    Single(String),
}

/// Parses [`terraform_display_label`] output into type / name when possible.
pub fn terraform_label_parts(display: &str) -> TerraformLabelParts {
    let display = display.trim();
    if display.is_empty() {
        return TerraformLabelParts::Single(String::new());
    }

    if let Some(rest) = display.strip_prefix("provider.") {
        return TerraformLabelParts::TwoLines {
            resource_type: "provider".to_string(),
            resource_name: rest.to_string(),
        };
    }

    if let Some(dot_pos) = display.rfind('.') {
        let resource_type = display[..dot_pos].trim();
        let resource_name = display[dot_pos + 1..].trim();
        if !resource_type.is_empty() && !resource_name.is_empty() {
            return TerraformLabelParts::TwoLines {
                resource_type: resource_type.to_string(),
                resource_name: resource_name.to_string(),
            };
        }
    }

    TerraformLabelParts::Single(display.to_string())
}

fn encoded_flow_label(parts: &TerraformLabelParts) -> String {
    match parts {
        TerraformLabelParts::TwoLines {
            resource_type,
            resource_name,
        } => format!("{resource_type}\n{resource_name}"),
        TerraformLabelParts::Single(s) => s.clone(),
    }
}

fn estimated_node_size(parts: &TerraformLabelParts) -> (f32, f32) {
    match parts {
        TerraformLabelParts::TwoLines {
            resource_type,
            resource_name,
        } => {
            let w_type = resource_type.chars().count() as f32 * LINE_TYPE_W_PER_CHAR;
            let w_name = resource_name.chars().count() as f32 * LINE_NAME_W_PER_CHAR;
            let width = (w_type.max(w_name) + NODE_PAD_X).clamp(NODE_WIDTH_MIN, NODE_WIDTH_MAX);
            (width, TWO_LINE_HEIGHT)
        }
        TerraformLabelParts::Single(line) => {
            let width =
                (line.chars().count() as f32 * LINE_NAME_W_PER_CHAR + NODE_PAD_X)
                    .clamp(NODE_WIDTH_MIN, NODE_WIDTH_MAX);
            (width, SINGLE_LINE_HEIGHT)
        }
    }
}

/// Display text only: `resource_type.name` or `provider.<type>` for Terraform graph DOT ids.
pub fn terraform_display_label(dot_node_id: &str) -> String {
    let mut s = dot_node_id.trim();

    if let Some(rest) = s.strip_prefix("[root]") {
        s = rest.trim_start();
    }

    if let Some(open) = s.find(" (\"") {
        let tail = &s[open..];
        if tail.starts_with(" (expand)") || tail.starts_with(" (close)") {
            s = &s[..open];
        }
    } else if let Some(open) = s.rfind(" (") {
        let tail = &s[open..];
        if tail.starts_with(" (expand)")
            || tail.starts_with(" (close)")
            || tail.starts_with(" (destroy)")
        {
            s = &s[..open];
        }
    }

    if let Some(provider_rest) = s.strip_prefix("provider[\"") {
        let mut addr = provider_rest;
        if let Some(end) = addr.find('"') {
            addr = &addr[..end];
        }
        if let Some(provider_type) = addr.rsplit('/').next() {
            return format!("provider.{provider_type}");
        }
    }

    s.to_string()
}

fn order_layers_barycenter(graph: &Graph<String, ()>, layers: &mut Vec<Vec<NodeIndex>>) {
    const PASSES: usize = 24;

    if layers.len() <= 1 {
        return;
    }

    for _ in 0..PASSES {
        for layer_idx in 1..layers.len() {
            let prev = &layers[layer_idx - 1];
            let pos: HashMap<NodeIndex, f32> = prev
                .iter()
                .enumerate()
                .map(|(slot, &node)| (node, slot as f32))
                .collect();

            layers[layer_idx].sort_by(|&a, &b| {
                let ba = barycenter_down(graph, &pos, a);
                let bb = barycenter_down(graph, &pos, b);
                ba.partial_cmp(&bb)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        terraform_display_label(graph[a].as_str())
                            .cmp(&terraform_display_label(graph[b].as_str()))
                    })
            });
        }

        for layer_idx in (0..layers.len().saturating_sub(1)).rev() {
            let next = &layers[layer_idx + 1];
            let pos: HashMap<NodeIndex, f32> = next
                .iter()
                .enumerate()
                .map(|(slot, &node)| (node, slot as f32))
                .collect();

            layers[layer_idx].sort_by(|&a, &b| {
                let ba = barycenter_up(graph, &pos, a);
                let bb = barycenter_up(graph, &pos, b);
                ba.partial_cmp(&bb)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        terraform_display_label(graph[a].as_str())
                            .cmp(&terraform_display_label(graph[b].as_str()))
                    })
            });
        }
    }
}

fn barycenter_down(
    graph: &Graph<String, ()>,
    prev_pos: &HashMap<NodeIndex, f32>,
    node: NodeIndex,
) -> f32 {
    let preds: Vec<NodeIndex> = graph
        .edges_directed(node, Incoming)
        .map(|edge| edge.source())
        .collect();
    if preds.is_empty() {
        return 0.0;
    }
    let sum: f32 = preds
        .iter()
        .filter_map(|pred| prev_pos.get(pred).copied())
        .sum();
    let count = preds.len() as f32;
    if count > 0.0 {
        sum / count
    } else {
        0.0
    }
}

fn barycenter_up(
    graph: &Graph<String, ()>,
    next_pos: &HashMap<NodeIndex, f32>,
    node: NodeIndex,
) -> f32 {
    let succs: Vec<NodeIndex> = graph
        .edges_directed(node, Outgoing)
        .map(|edge| edge.target())
        .collect();
    if succs.is_empty() {
        return 0.0;
    }
    let sum: f32 = succs
        .iter()
        .filter_map(|succ| next_pos.get(succ).copied())
        .sum();
    let count = succs.len() as f32;
    if count > 0.0 {
        sum / count
    } else {
        0.0
    }
}

#[derive(Debug)]
pub struct FlowGraphModel {
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdge>,
}

pub fn layout_flow_graph(graph: &Graph<String, ()>) -> anyhow::Result<FlowGraphModel> {
    if !is_dag(graph) {
        anyhow::bail!("Graph contains a cycle");
    }

    let node_count = graph.node_count();
    let mut layer_index: Vec<usize> = vec![0; node_count];

    let topo = petgraph::algo::toposort(graph, None)
        .map_err(|_| anyhow!("Graph contains a cycle"))?;

    for node in topo {
        let mut layer = 0_usize;
        for edge in graph.edges_directed(node, Incoming) {
            let pred = edge.source().index();
            layer = layer.max(layer_index[pred] + 1);
        }
        layer_index[node.index()] = layer;
    }

    let max_layer = layer_index.iter().copied().max().unwrap_or(0);
    let mut layers: Vec<Vec<NodeIndex>> = vec![Vec::new(); max_layer + 1];
    for node in graph.node_indices() {
        let layer = layer_index[node.index()];
        layers[layer].push(node);
    }

    for layer_nodes in &mut layers {
        layer_nodes.sort_by_key(|&idx| terraform_display_label(graph[idx].as_str()));
    }

    order_layers_barycenter(graph, &mut layers);

    let mut cum_y = 0.0_f32;
    let mut nodes = Vec::with_capacity(node_count);
    for layer_nodes in &layers {
        let count = layer_nodes.len();
        if count == 0 {
            continue;
        }

        let parts_per_node: Vec<TerraformLabelParts> = layer_nodes
            .iter()
            .map(|&idx| terraform_label_parts(&terraform_display_label(graph[idx].as_str())))
            .collect();

        let max_layer_height = parts_per_node
            .iter()
            .map(|parts| estimated_node_size(parts).1)
            .fold(0.0_f32, f32::max);

        let max_content_width = parts_per_node
            .iter()
            .map(|parts| estimated_node_size(parts).0)
            .fold(0.0_f32, f32::max);

        let cell_width = max_content_width;
        let row_width =
            count as f32 * cell_width + (count.saturating_sub(1) as f32) * NODE_GAP;
        let left_edge = -row_width / 2.0;

        let y = cum_y;
        cum_y += max_layer_height + VERTICAL_STACK_GAP;

        for (i, &node_idx) in layer_nodes.iter().enumerate() {
            let full_id = graph[node_idx].clone();
            let parts = parts_per_node[i].clone();
            let label = encoded_flow_label(&parts);
            let (node_w, node_h) = estimated_node_size(&parts);
            let x = left_edge + i as f32 * (cell_width + NODE_GAP);

            nodes.push(
                FlowNode::new(full_id.clone(), x, y)
                    .label(label)
                    .size(node_w, node_h)
                    .handles(vec![
                        HandleDef::target(HandlePosition::Top),
                        HandleDef::source(HandlePosition::Bottom),
                    ]),
            );
        }
    }

    let mut edges = Vec::new();
    for (edge_idx, edge) in graph.edge_references().enumerate() {
        let source_id = graph[edge.source()].clone();
        let target_id = graph[edge.target()].clone();
        edges.push(FlowEdge::new(
            format!("e{}", edge_idx),
            source_id,
            target_id,
        ));
    }

    Ok(FlowGraphModel { nodes, edges })
}

pub struct ParsedDot {
    pub graph: Graph<String, ()>,
}

pub fn parse_dot_to_digraph(dot: &str) -> anyhow::Result<ParsedDot> {
    Parser::new(dot)
        .parse()
        .with_context(|| "failed to parse DOT digraph".to_string())
}

pub fn is_dag(graph: &Graph<String, ()>) -> bool {
    !is_cyclic_directed(graph)
}

struct Parser<'a> {
    input: &'a str,
    offset: usize,
    graph: Graph<String, ()>,
    nodes: HashMap<String, NodeIndex>,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            offset: 0,
            graph: Graph::new(),
            nodes: HashMap::new(),
        }
    }

    fn parse(mut self) -> anyhow::Result<ParsedDot> {
        self.skip_ws();
        if self.try_keyword("strict") {
            self.skip_ws();
        }
        if !self.try_keyword("digraph") {
            anyhow::bail!("expected `digraph` (optional leading `strict`)");
        }
        self.skip_ws();
        self.skip_optional_graph_id()?;
        self.expect_byte(b'{')
            .context("expected `{` after `digraph`")?;
        self.parse_scope()
            .context("expected content inside top-level `digraph {{`")?;
        self.skip_ws();
        if !self.is_eof() {
            anyhow::bail!("unexpected trailing content after digraph");
        }
        Ok(ParsedDot {
            graph: self.graph,
        })
    }

    fn skip_optional_graph_id(&mut self) -> anyhow::Result<()> {
        self.skip_ws();
        if self.peek_char() == Some('{') {
            return Ok(());
        }
        let _ = self.parse_node_id().context("invalid graph id after `digraph`")?;
        Ok(())
    }

    /// Parses statements until a closing `}` for the current scope (digraph body or subgraph).
    fn parse_scope(&mut self) -> anyhow::Result<()> {
        loop {
            self.skip_ws();
            match self.peek_byte() {
                Some(b'}') => {
                    self.offset += 1;
                    return Ok(());
                }
                None => anyhow::bail!("unclosed scope: expected `}}`"),
                _ => self.parse_statement()?,
            }
        }
    }

    fn parse_statement(&mut self) -> anyhow::Result<()> {
        self.skip_ws();
        if self.peek_byte() == Some(b'}') {
            return Ok(());
        }

        if self.try_keyword("subgraph") {
            self.skip_ws();
            self.skip_optional_graph_id()?;
            self.expect_byte(b'{')
                .context("expected `{{` after `subgraph`")?;
            return self.parse_scope();
        }

        let first = self
            .parse_node_id()
            .context("expected node id, subgraph, or end of scope")?;

        self.skip_ws();
        if self.peek_byte() == Some(b'=') {
            self.skip_graph_assignment()?;
            return Ok(());
        }
        if self.try_arrow() {
            let target = self
                .parse_node_id()
                .context("expected target node id after `->`")?;
            self.ensure_edge(first, target)?;
            self.skip_ws();
            self.skip_optional_edge_attributes()?;
            self.skip_ws();
            self.skip_optional_semicolon();
            return Ok(());
        }

        self.skip_ws();
        if self.peek_byte() == Some(b'[') {
            self.skip_balanced_square_brackets()?;
        }
        self.skip_ws();
        self.skip_optional_semicolon();
        self.ensure_node(first)?;
        Ok(())
    }

    fn skip_graph_assignment(&mut self) -> anyhow::Result<()> {
        self.expect_byte(b'=')?;
        self.skip_ws();
        self.skip_attribute_value()?;
        self.skip_ws();
        self.skip_optional_semicolon();
        Ok(())
    }

    fn skip_attribute_value(&mut self) -> anyhow::Result<()> {
        match self.peek_byte() {
            Some(b'"') => {
                self.parse_quoted_id()?;
            }
            Some(_) => {
                self.parse_unquoted_id();
            }
            None => anyhow::bail!("unexpected end of input in assignment"),
        }
        Ok(())
    }

    fn skip_optional_edge_attributes(&mut self) -> anyhow::Result<()> {
        if self.peek_byte() == Some(b'[') {
            self.skip_balanced_square_brackets()?;
        }
        Ok(())
    }

    fn skip_balanced_square_brackets(&mut self) -> anyhow::Result<()> {
        self.expect_byte(b'[')?;
        let bytes = self.input.as_bytes();
        let mut depth = 1_u32;
        while self.offset < bytes.len() && depth > 0 {
            match bytes[self.offset] {
                b'[' => {
                    depth += 1;
                    self.offset += 1;
                }
                b']' => {
                    depth -= 1;
                    self.offset += 1;
                }
                b'"' => {
                    self.offset += 1;
                    while self.offset < bytes.len() {
                        let b = bytes[self.offset];
                        self.offset += 1;
                        if b == b'"' {
                            break;
                        }
                    }
                }
                _ => self.offset += 1,
            }
        }
        if depth != 0 {
            anyhow::bail!("unterminated `[` attribute list");
        }
        Ok(())
    }

    fn try_arrow(&mut self) -> bool {
        let rest = &self.input[self.offset..];
        if rest.starts_with("->") {
            self.offset += 2;
            return true;
        }
        false
    }

    fn ensure_node(&mut self, id: String) -> anyhow::Result<()> {
        self.intern_node(id);
        Ok(())
    }

    fn ensure_edge(&mut self, from: String, to: String) -> anyhow::Result<()> {
        let from_idx = self.intern_node(from);
        let to_idx = self.intern_node(to);
        self.graph.add_edge(from_idx, to_idx, ());
        Ok(())
    }

    fn intern_node(&mut self, id: String) -> NodeIndex {
        if let Some(&idx) = self.nodes.get(&id) {
            return idx;
        }
        let idx = self.graph.add_node(id.clone());
        self.nodes.insert(id, idx);
        idx
    }

    fn parse_node_id(&mut self) -> anyhow::Result<String> {
        self.skip_ws();
        match self.peek_byte() {
            Some(b'"') => self.parse_quoted_id(),
            Some(c) if is_id_start(c) => Ok(self.parse_unquoted_id()),
            _ => Err(anyhow!(
                "expected quoted or unquoted node id at byte {}",
                self.offset
            )),
        }
    }

    fn parse_quoted_id(&mut self) -> anyhow::Result<String> {
        self.expect_byte(b'"')
            .context("expected opening `\"` for quoted node id")?;
        let mut out = String::new();
        let bytes = self.input.as_bytes();
        while self.offset < bytes.len() {
            let b = bytes[self.offset];
            if b == b'"' {
                self.offset += 1;
                return Ok(out);
            }
            if b == b'\\' {
                self.offset += 1;
                if self.offset >= bytes.len() {
                    return Err(anyhow!(
                        "unterminated escape at end of string starting near byte {}",
                        self.offset.saturating_sub(2)
                    ));
                }
                match bytes[self.offset] {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    other => out.push(other as char),
                }
                self.offset += 1;
                continue;
            }
            let ch = self.input[self.offset..]
                .chars()
                .next()
                .ok_or_else(|| anyhow!("unexpected end inside quoted id"))?;
            out.push(ch);
            self.offset += ch.len_utf8();
        }
        Err(anyhow!("unterminated quoted string"))
    }

    fn parse_unquoted_id(&mut self) -> String {
        let start = self.offset;
        let bytes = self.input.as_bytes();
        while self.offset < bytes.len() && is_id_continue(bytes[self.offset]) {
            self.offset += 1;
        }
        self.input[start..self.offset].to_string()
    }

    fn skip_optional_semicolon(&mut self) {
        self.skip_ws();
        if self.peek_byte() == Some(b';') {
            self.offset += 1;
        }
    }

    fn skip_ws(&mut self) {
        let bytes = self.input.as_bytes();
        while self.offset < bytes.len() {
            match bytes[self.offset] {
                b' ' | b'\t' | b'\n' | b'\r' => self.offset += 1,
                _ => break,
            }
        }
    }

    fn peek_byte(&self) -> Option<u8> {
        self.input.as_bytes().get(self.offset).copied()
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.offset..].chars().next()
    }

    fn is_eof(&self) -> bool {
        self.offset >= self.input.len()
    }

    fn expect_byte(&mut self, expected: u8) -> anyhow::Result<()> {
        self.skip_ws();
        match self.peek_byte() {
            Some(b) if b == expected => {
                self.offset += 1;
                Ok(())
            }
            Some(b) => Err(anyhow!(
                "expected byte {:?}, found {:?} at offset {}",
                expected as char,
                b as char,
                self.offset
            )),
            None => Err(anyhow!(
                "unexpected end of input, expected {:?}",
                expected as char
            )),
        }
    }

    fn try_keyword(&mut self, word: &str) -> bool {
        self.skip_ws();
        let rest = &self.input[self.offset..];
        if rest.len() < word.len() {
            return false;
        }
        let prefix = &rest[..word.len()];
        if prefix != word {
            return false;
        }
        let boundary_ok = rest
            .as_bytes()
            .get(word.len())
            .map_or(true, |b| !is_id_continue(*b));
        if !boundary_ok {
            return false;
        }
        self.offset += word.len();
        true
    }
}

fn is_id_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_id_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(b, b'_' | b'.' | b'-' | b'[' | b']' | b'/' | b':' | b'(' | b')')
}

#[cfg(test)]
mod label_parts_tests {
    use super::*;

    #[test]
    fn splits_null_resource() {
        match terraform_label_parts("null_resource.deploy") {
            TerraformLabelParts::TwoLines {
                resource_type,
                resource_name,
            } => {
                assert_eq!(resource_type, "null_resource");
                assert_eq!(resource_name, "deploy");
            }
            TerraformLabelParts::Single(_) => panic!("expected two lines"),
        }
    }

    #[test]
    fn provider_splits_to_type_provider() {
        match terraform_label_parts("provider.null") {
            TerraformLabelParts::TwoLines {
                resource_type,
                resource_name,
            } => {
                assert_eq!(resource_type, "provider");
                assert_eq!(resource_name, "null");
            }
            TerraformLabelParts::Single(_) => panic!("expected two lines"),
        }
    }
}

#[cfg(test)]
mod label_tests {
    use super::terraform_display_label;

    #[test]
    fn strips_root_brackets_and_expand_suffix() {
        assert_eq!(
            terraform_display_label("[root] null_resource.frontend_build (expand)"),
            "null_resource.frontend_build"
        );
    }

    #[test]
    fn provider_registry_address_maps_to_provider_null() {
        let id = "[root] provider[\"registry.terraform.io/hashicorp/null\"]";
        assert_eq!(terraform_display_label(id), "provider.null");
    }

    #[test]
    fn plain_labels_untouched() {
        assert_eq!(terraform_display_label("a"), "a");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE: &str = r#"
digraph {
  "a" -> "b";
  "b" -> "c";
}
"#;

    const TERRAFORM_STYLE: &str = r#"
digraph {
	compound = "true"
	newrank = "true"
	subgraph "root" {
		"[root] null_resource.first (expand)" [label = "null_resource.first", shape = "box"]
		"[root] null_resource.second (expand)" [label = "null_resource.second", shape = "box"]
		"[root] null_resource.first (expand)" -> "[root] null_resource.second (expand)"
	}
}
"#;

    #[test]
    fn parses_terraform_subgraph() {
        let parsed = parse_dot_to_digraph(TERRAFORM_STYLE).expect("parse");
        assert!(parsed.graph.node_count() >= 2);
        assert!(is_dag(&parsed.graph));
    }

    #[test]
    fn parses_chain_dag() {
        let parsed = parse_dot_to_digraph(SIMPLE).expect("parse");
        assert_eq!(parsed.graph.node_count(), 3);
        assert!(is_dag(&parsed.graph));
    }

    #[test]
    fn detects_two_node_cycle() {
        let dot = r#"
digraph {
  "a" -> "b";
  "b" -> "a";
}
"#;
        let parsed = parse_dot_to_digraph(dot).expect("parse");
        assert_eq!(parsed.graph.node_count(), 2);
        assert!(!is_dag(&parsed.graph));
        assert!(is_cyclic_directed(&parsed.graph));
    }

    #[test]
    fn layout_same_layer_aligns_y() {
        let dot = r#"
digraph {
  "left" -> "down";
  "right" -> "down";
}
"#;
        let parsed = parse_dot_to_digraph(dot).expect("parse");
        let model = layout_flow_graph(&parsed.graph).expect("layout");
        let y_left = model
            .nodes
            .iter()
            .find(|n| n.id.as_ref() == "left")
            .unwrap()
            .position
            .y;
        let y_right = model
            .nodes
            .iter()
            .find(|n| n.id.as_ref() == "right")
            .unwrap()
            .position
            .y;
        assert!(
            (y_left - y_right).abs() < 0.01,
            "same layer must share y: left={y_left} right={y_right}"
        );
    }

    #[test]
    fn layout_chain_y_increases_downstream() {
        let parsed = parse_dot_to_digraph(SIMPLE).expect("parse");
        let model = layout_flow_graph(&parsed.graph).expect("layout");
        let y = |id: &str| {
            model
                .nodes
                .iter()
                .find(|node| node.id.as_ref() == id)
                .map(|node| node.position.y)
                .expect("node id")
        };
        assert!(y("a") < y("b"), "a should be above b");
        assert!(y("b") < y("c"), "b should be above c");
    }

    #[test]
    fn layout_rejects_cycle() {
        let dot = r#"
digraph {
  "a" -> "b";
  "b" -> "a";
}
"#;
        let parsed = parse_dot_to_digraph(dot).expect("parse");
        let err = layout_flow_graph(&parsed.graph).expect_err("cycle");
        assert_eq!(err.to_string(), "Graph contains a cycle");
    }
}
