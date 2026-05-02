use std::collections::HashMap;

use anyhow::{anyhow, Context};
use petgraph::algo::is_cyclic_directed;
use petgraph::graph::NodeIndex;
use petgraph::Graph;

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
        self.parse_body()?;
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

    fn parse_body(&mut self) -> anyhow::Result<()> {
        loop {
            self.skip_ws();
            match self.peek_byte() {
                Some(b'}') => {
                    self.offset += 1;
                    self.skip_ws();
                    if !self.is_eof() {
                        anyhow::bail!("unexpected trailing content after closing `}}`");
                    }
                    return Ok(());
                }
                None => anyhow::bail!("unclosed digraph: expected `}}`"),
                _ => self.parse_statement()?,
            }
        }
    }

    fn parse_statement(&mut self) -> anyhow::Result<()> {
        self.skip_ws();
        if self.peek_byte() == Some(b'}') {
            return Ok(());
        }

        let first = self
            .parse_node_id()
            .context("expected node id or end of digraph")?;

        self.skip_ws();
        if self.try_arrow() {
            let target = self
                .parse_node_id()
                .context("expected target node id after `->`")?;
            self.ensure_edge(first, target)?;
            self.skip_ws();
            self.skip_optional_semicolon();
            return Ok(());
        }

        self.skip_optional_semicolon();
        self.ensure_node(first)?;
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
        let start = self.offset;
        let bytes = self.input.as_bytes();
        while self.offset < bytes.len() {
            let b = bytes[self.offset];
            if b == b'"' {
                let inner = self.input[start..self.offset].to_string();
                self.offset += 1;
                return Ok(inner);
            }
            if b == b'\\' {
                return Err(anyhow!(
                    "escape sequences in quoted node ids are not supported at byte {}",
                    self.offset
                ));
            }
            self.offset += 1;
        }
        Err(anyhow!("unterminated string starting at byte {}", start.saturating_sub(1)))
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
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-'
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
}
