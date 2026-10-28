//! Wires between boxes, shared by every node graph (Patches, Designer networks): one output can feed many inputs,
//! an input takes one wire, and nothing may loop back on itself.

use serde::{Deserialize, Serialize};

/// The output of `from` feeds input `input` of `to`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wire {
    pub from: u32,
    pub to: u32,
    pub input: usize,
}

/// Wires `from` into input `input` of `to` (replacing what was there). Refused (false) when it would make a loop,
/// `from` is `to`, or the input doesn't exist (`inputs` = how many `to` has).
pub fn connect(wires: &mut Vec<Wire>, from: u32, to: u32, input: usize, inputs: usize) -> bool {
    if from == to || input >= inputs || feeds(wires, to, from) {
        return false;
    }
    disconnect(wires, to, input);
    wires.push(Wire { from, to, input });
    true
}

pub fn disconnect(wires: &mut Vec<Wire>, to: u32, input: usize) {
    wires.retain(|w| !(w.to == to && w.input == input));
}

/// What's wired into input `input` of `to`.
pub fn source_of(wires: &[Wire], to: u32, input: usize) -> Option<u32> {
    wires.iter().find(|w| w.to == to && w.input == input).map(|w| w.from)
}

/// Whether `a`'s picture reaches `b` (so wiring b into a would loop).
pub fn feeds(wires: &[Wire], a: u32, b: u32) -> bool {
    let mut todo = vec![a];
    let mut seen = Vec::new();
    while let Some(n) = todo.pop() {
        if n == b {
            return true;
        }
        if !seen.contains(&n) {
            seen.push(n);
            todo.extend(wires.iter().filter(|w| w.from == n).map(|w| w.to));
        }
    }
    false
}

/// Drops every wire touching `id` (a removed box).
pub fn remove(wires: &mut Vec<Wire>, id: u32) {
    wires.retain(|w| w.from != id && w.to != id);
}

/// Boxes in an order where every box comes after the ones feeding it (sources first). Boxes in a cycle (which
/// `connect` refuses) are left out.
pub fn order(ids: &[u32], wires: &[Wire]) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    let mut left: Vec<u32> = ids.to_vec();
    while !left.is_empty() {
        let ready: Vec<u32> = left.iter().copied().filter(|id| wires.iter().filter(|w| w.to == *id).all(|w| out.contains(&w.from) || !ids.contains(&w.from))).collect();
        if ready.is_empty() {
            break;
        }
        left.retain(|id| !ready.contains(id));
        out.extend(ready);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wires_replace_loops_are_refused_and_order_puts_sources_first() {
        let mut w = Vec::new();
        assert!(connect(&mut w, 1, 2, 0, 1));
        assert!(connect(&mut w, 2, 3, 0, 2));
        assert!(!connect(&mut w, 3, 1, 0, 1), "a loop");
        assert!(!connect(&mut w, 1, 3, 2, 2), "no such input");
        assert!(connect(&mut w, 1, 3, 1, 2));
        assert!(connect(&mut w, 4, 2, 0, 1), "replaces 1 to 2");
        assert_eq!(source_of(&w, 2, 0), Some(4));
        assert_eq!(order(&[3, 2, 1, 4], &w), vec![1, 4, 2, 3]);
        remove(&mut w, 2);
        assert_eq!(w.len(), 1);
        disconnect(&mut w, 3, 1);
        assert!(w.is_empty());
    }
}
