// Finds the cheapest chain of engines from one format to another (Dijkstra over formats).

use crate::engine::{Engine, Engines};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::sync::Arc;

pub const MAX_STEPS: usize = 3;

#[derive(Clone)]
pub struct Hop {
    pub engine: Arc<dyn Engine>,
    pub from: &'static str,
    pub to: &'static str,
}

/// (engine, target format, cost)
type Out = (Arc<dyn Engine>, &'static str, u32);

pub struct Planner {
    from: HashMap<&'static str, Vec<Out>>,
}

impl Planner {
    pub fn new(engines: &Engines) -> Self {
        let mut from: HashMap<_, Vec<_>> = HashMap::new();
        for e in engines {
            for edge in e.edges() {
                from.entry(edge.from)
                    .or_default()
                    .push((e.clone(), edge.to, edge.cost));
            }
        }
        Planner { from }
    }

    pub fn plan(&self, from: &str, to: &str) -> Option<Vec<Hop>> {
        let from = crate::formats::by_id(from)?.id;
        // Same format: only an explicit self edge (re-encode, compress).
        if from == to {
            let (e, t, _) = self
                .from
                .get(from)?
                .iter()
                .filter(|(_, t, _)| *t == to)
                .min_by_key(|x| x.2)?;
            return Some(vec![Hop {
                engine: e.clone(),
                from,
                to: t,
            }]);
        }
        // State is (format, hops so far): a dearer start may be the only one that fits in MAX_STEPS.
        let mut dist: HashMap<(&str, usize), u32> = HashMap::from([((from, 0), 0)]);
        let mut prev: HashMap<(&str, usize), Hop> = HashMap::new();
        let mut queue = BinaryHeap::from([Reverse((0u32, 0usize, from))]);
        let mut end = None;
        while let Some(Reverse((cost, hops, node))) = queue.pop() {
            if cost > dist[&(node, hops)] {
                continue;
            }
            if node == to {
                end = Some(hops);
                break;
            }
            if hops >= MAX_STEPS {
                continue;
            }
            for (engine, next, c) in self.from.get(node).into_iter().flatten() {
                let n = cost + c;
                let key = (*next, hops + 1);
                if *next == node || dist.get(&key).is_some_and(|d| *d <= n) {
                    continue;
                }
                dist.insert(key, n);
                prev.insert(
                    key,
                    Hop {
                        engine: engine.clone(),
                        from: node_static(node),
                        to: next,
                    },
                );
                queue.push(Reverse((n, hops + 1, *next)));
            }
        }
        let mut steps = vec![];
        let (mut node, mut hops) = (to, end?);
        while hops > 0 {
            let hop = prev[&(node, hops)].clone();
            node = hop.from;
            hops -= 1;
            steps.push(hop);
        }
        steps.reverse();
        Some(steps)
    }

    /// Every format reachable from `from`, cheapest first.
    pub fn targets(&self, from: &str) -> Vec<&'static str> {
        let mut out: Vec<(&'static str, u32)> = crate::formats::KNOWN
            .iter()
            .filter(|f| f.writable && f.id != from)
            .filter_map(|f| {
                let cost = self.plan(from, f.id)?.iter().map(cost_of).sum();
                Some((f.id, cost))
            })
            .collect();
        out.sort_by_key(|(_, c)| *c);
        out.into_iter().map(|(id, _)| id).collect()
    }
}

fn cost_of(h: &Hop) -> u32 {
    h.engine
        .edges()
        .iter()
        .find(|e| e.from == h.from && e.to == h.to)
        .map_or(0, |e| e.cost)
}

fn node_static(id: &str) -> &'static str {
    crate::formats::by_id(id).map_or("", |f| f.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Edge, Step, edge};
    use std::path::PathBuf;

    struct Fake(&'static str, Vec<Edge>);
    impl Engine for Fake {
        fn id(&self) -> &'static str {
            self.0
        }
        fn edges(&self) -> &[Edge] {
            &self.1
        }
        fn convert(&self, _: &Step) -> anyhow::Result<Vec<PathBuf>> {
            Ok(vec![])
        }
    }

    #[test]
    fn picks_the_cheapest_chain_within_three_steps() {
        let engines: Engines = vec![
            Arc::new(Fake(
                "a",
                vec![edge("hwp", "pdf", 10), edge("pdf", "png", 10)],
            )),
            Arc::new(Fake(
                "b",
                vec![
                    edge("hwp", "png", 25),
                    edge("png", "jpg", 10),
                    edge("jpg", "webp", 10),
                ],
            )),
        ];
        let p = Planner::new(&engines);
        let ids = |r: Vec<Hop>| r.iter().map(|h| (h.engine.id(), h.to)).collect::<Vec<_>>();
        assert_eq!(
            ids(p.plan("hwp", "png").unwrap()),
            [("a", "pdf"), ("a", "png")]
        );
        assert_eq!(
            ids(p.plan("hwp", "jpg").unwrap()),
            [("a", "pdf"), ("a", "png"), ("b", "jpg")]
        );
        // Four hops via the cheap path; only the 25-cost start fits in three.
        assert_eq!(
            ids(p.plan("hwp", "webp").unwrap()),
            [("b", "png"), ("b", "jpg"), ("b", "webp")]
        );
        assert!(p.plan("png", "png").is_none());
        assert!(p.plan("png", "hwp").is_none());
    }
}
