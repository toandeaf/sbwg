//! Grid A* pathfinding over walkable tiles (8-connectivity, no corner-cutting).

use crate::map::Map;
use bevy::prelude::*;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

/// A* frontier node, ordered by f-cost for a min-heap.
#[derive(Eq, PartialEq)]
struct Node {
    f: i32,
    pos: IVec2,
}

impl Ord for Node {
    fn cmp(&self, other: &Self) -> Ordering {
        other.f.cmp(&self.f) // reversed → BinaryHeap behaves as a min-heap
    }
}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Octile heuristic (orthogonal step = 10, diagonal = 14).
fn heuristic(a: IVec2, b: IVec2) -> i32 {
    let dx = (a.x - b.x).abs();
    let dy = (a.y - b.y).abs();
    let (lo, hi) = if dx < dy { (dx, dy) } else { (dy, dx) };
    14 * lo + 10 * (hi - lo)
}

/// Find a tile path from `start` to `goal` over walkable tiles, inclusive of
/// both ends. `None` if either end is blocked or no route exists.
pub fn find_path(map: &Map, start: IVec2, goal: IVec2) -> Option<Vec<IVec2>> {
    if !map.is_walkable(start.x, start.y) || !map.is_walkable(goal.x, goal.y) {
        return None;
    }
    if start == goal {
        return Some(vec![start]);
    }

    const STEPS: [(IVec2, i32); 8] = [
        (IVec2::new(1, 0), 10),
        (IVec2::new(-1, 0), 10),
        (IVec2::new(0, 1), 10),
        (IVec2::new(0, -1), 10),
        (IVec2::new(1, 1), 14),
        (IVec2::new(1, -1), 14),
        (IVec2::new(-1, 1), 14),
        (IVec2::new(-1, -1), 14),
    ];

    let mut open = BinaryHeap::new();
    let mut g: HashMap<IVec2, i32> = HashMap::new();
    let mut came: HashMap<IVec2, IVec2> = HashMap::new();
    let mut closed: HashSet<IVec2> = HashSet::new();

    g.insert(start, 0);
    open.push(Node { f: heuristic(start, goal), pos: start });

    while let Some(Node { pos, .. }) = open.pop() {
        if pos == goal {
            return Some(reconstruct(&came, goal));
        }
        if !closed.insert(pos) {
            continue; // a better route to `pos` was already finalised
        }
        let cur_g = g[&pos];
        for (step, cost) in STEPS {
            let next = pos + step;
            if !map.is_walkable(next.x, next.y) {
                continue;
            }
            // No diagonal corner-cutting through blocked tiles.
            if step.x != 0
                && step.y != 0
                && (!map.is_walkable(pos.x + step.x, pos.y) || !map.is_walkable(pos.x, pos.y + step.y))
            {
                continue;
            }
            let ng = cur_g + cost;
            if ng < *g.get(&next).unwrap_or(&i32::MAX) {
                came.insert(next, pos);
                g.insert(next, ng);
                open.push(Node { f: ng + heuristic(next, goal), pos: next });
            }
        }
    }
    None
}

fn reconstruct(came: &HashMap<IVec2, IVec2>, mut cur: IVec2) -> Vec<IVec2> {
    let mut path = vec![cur];
    while let Some(&prev) = came.get(&cur) {
        cur = prev;
        path.push(cur);
    }
    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Map;

    #[test]
    fn path_connects_walkable_tiles() {
        let map = Map::default();
        let mut start = None;
        let mut goal = None;
        for y in 0..map.height {
            for x in 0..map.width {
                if map.is_walkable(x, y) {
                    let t = IVec2::new(x, y);
                    start.get_or_insert(t);
                    goal = Some(t);
                }
            }
        }
        let (start, goal) = (start.unwrap(), goal.unwrap());
        let path = find_path(&map, start, goal).expect("expected a path");
        assert_eq!(*path.first().unwrap(), start);
        assert_eq!(*path.last().unwrap(), goal);
        for w in path.windows(2) {
            let d = w[1] - w[0];
            assert!(d.x.abs() <= 1 && d.y.abs() <= 1 && d != IVec2::ZERO);
            assert!(map.is_walkable(w[1].x, w[1].y));
        }
    }

    #[test]
    fn no_path_to_blocked_goal() {
        let map = Map::default();
        let start = map.find_walkable_near(Vec2::new(map.width as f32 / 2.0, map.height as f32 / 2.0), 0.0);
        let start = IVec2::new(start.x.floor() as i32, start.y.floor() as i32);
        let mut blocked = None;
        for y in 0..map.height {
            for x in 0..map.width {
                if !map.is_walkable(x, y) {
                    blocked = Some(IVec2::new(x, y));
                }
            }
        }
        if let Some(b) = blocked {
            assert!(find_path(&map, start, b).is_none());
        }
    }
}
