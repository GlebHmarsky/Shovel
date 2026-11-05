//! Port of the baseline's stable, paged graph. Stashes annotate their actual
//! base without taking ownership of an ordinary branch lane.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_LANES: usize = 64;
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Cursor {
    pub lanes: Vec<Lane>,
    pub next_color: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Lane {
    pub hash: String,
    pub color: usize,
    pub is_stash: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Line {
    pub from: usize,
    pub from_y: f32,
    pub to: usize,
    pub to_y: f32,
    pub color: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Row {
    pub node: usize,
    pub color: usize,
    pub width: usize,
    pub is_merge: bool,
    pub is_stash: bool,
    pub lines: Vec<Line>,
}
#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("history exceeds the supported graph width")]
    TooWide,
    #[error("history is not a valid topological sequence")]
    InvalidTopology,
}
fn allocate(lanes: &mut Vec<Lane>, lane: Lane) -> Result<usize, GraphError> {
    if let Some(index) = lanes.iter().position(|l| l.hash.is_empty()) {
        lanes[index] = lane;
        return Ok(index);
    }
    if lanes.len() >= MAX_LANES {
        return Err(GraphError::TooWide);
    }
    lanes.push(lane);
    Ok(lanes.len() - 1)
}

pub fn build<'a>(
    commits: impl IntoIterator<Item = (&'a str, &'a [String], bool)>,
    cursor: &Cursor,
) -> Result<(Vec<Row>, Cursor), GraphError> {
    let mut lanes = cursor.lanes.clone();
    let mut next_color = cursor.next_color;
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    if lanes.len() > MAX_LANES {
        return Err(GraphError::TooWide);
    }
    for (hash, parents, is_stash) in commits {
        if parents.len() > MAX_LANES {
            return Err(GraphError::TooWide);
        }
        if !seen.insert(hash) || parents.iter().any(|p| seen.contains(p.as_str())) {
            return Err(GraphError::InvalidTopology);
        }
        if is_stash && parents.len() == 1 {
            let mut lines = lanes
                .iter()
                .enumerate()
                .filter(|(_, l)| !l.hash.is_empty())
                .map(|(i, l)| Line {
                    from: i,
                    from_y: 0.,
                    to: i,
                    to_y: 1.,
                    color: l.color,
                })
                .collect::<Vec<_>>();
            let target = match lanes
                .iter()
                .position(|l| l.hash == parents[0] && !l.is_stash)
            {
                Some(i) => i,
                None => {
                    let i = allocate(
                        &mut lanes,
                        Lane {
                            hash: parents[0].clone(),
                            color: next_color,
                            is_stash: false,
                        },
                    )?;
                    next_color += 1;
                    i
                }
            };
            let node = lanes.len();
            if node >= MAX_LANES {
                return Err(GraphError::TooWide);
            }
            let color = next_color;
            next_color += 1;
            lines.push(Line {
                from: node,
                from_y: 0.5,
                to: target,
                to_y: 1.,
                color,
            });
            rows.push(Row {
                node,
                color,
                width: node + 1,
                is_merge: false,
                is_stash: true,
                lines,
            });
            continue;
        }
        let existing = lanes.iter().position(|l| l.hash == hash && !l.is_stash);
        let node = match existing {
            Some(i) => i,
            None => {
                let i = allocate(
                    &mut lanes,
                    Lane {
                        hash: hash.into(),
                        color: next_color,
                        is_stash: false,
                    },
                )?;
                next_color += 1;
                i
            }
        };
        let before = lanes.clone();
        let color = before[node].color;
        for lane in &mut lanes {
            if lane.hash == hash {
                lane.hash.clear();
                lane.is_stash = false;
            }
        }
        let mut unique = Vec::new();
        for parent in parents {
            if !unique.contains(parent) {
                unique.push(parent.clone());
            }
        }
        let mut outgoing = Vec::new();
        for (p, parent) in unique.iter().enumerate() {
            if p == 0 {
                lanes[node] = Lane {
                    hash: parent.clone(),
                    color,
                    is_stash,
                };
                outgoing.push(node);
            } else {
                let target = match lanes.iter().position(|l| l.hash == *parent) {
                    Some(i) => i,
                    None => {
                        let i = allocate(
                            &mut lanes,
                            Lane {
                                hash: parent.clone(),
                                color: next_color,
                                is_stash: false,
                            },
                        )?;
                        next_color += 1;
                        i
                    }
                };
                outgoing.push(target);
            }
        }
        let width = before.len().max(lanes.len());
        let mut lines = Vec::new();
        for (i, lane) in before
            .iter()
            .enumerate()
            .filter(|(_, l)| !l.hash.is_empty())
        {
            if lane.hash == hash {
                if existing.is_some() || i != node {
                    lines.push(Line {
                        from: i,
                        from_y: 0.,
                        to: node,
                        to_y: 0.5,
                        color: lane.color,
                    });
                }
            } else {
                lines.push(Line {
                    from: i,
                    from_y: 0.,
                    to: i,
                    to_y: 1.,
                    color: lane.color,
                });
            }
        }
        for lane in outgoing {
            lines.push(Line {
                from: node,
                from_y: 0.5,
                to: lane,
                to_y: 1.,
                color: lanes[lane].color,
            });
        }
        rows.push(Row {
            node,
            color,
            width,
            is_merge: unique.len() > 1,
            is_stash,
            lines,
        });
        while lanes.last().is_some_and(|l| l.hash.is_empty()) {
            lanes.pop();
        }
    }
    Ok((rows, Cursor { lanes, next_color }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn page_boundaries_preserve_graph_exactly() {
        let commits = [
            ("merge", vec!["left".into(), "right".into()], false),
            ("left", vec!["base".into()], false),
            ("right", vec!["base".into()], false),
            ("base", vec![], false),
        ];
        let (all, end) = build(
            commits.iter().map(|(h, p, s)| (*h, p.as_slice(), *s)),
            &Cursor::default(),
        )
        .unwrap();
        let (mut first, cursor) = build(
            commits[..2].iter().map(|(h, p, s)| (*h, p.as_slice(), *s)),
            &Cursor::default(),
        )
        .unwrap();
        let (last, split_end) = build(
            commits[2..].iter().map(|(h, p, s)| (*h, p.as_slice(), *s)),
            &cursor,
        )
        .unwrap();
        first.extend(last);
        assert_eq!(all, first);
        assert_eq!(end, split_end);
        assert!(end.lanes.is_empty());
    }
    #[test]
    fn many_stashes_share_base_without_reserving_more_lanes() {
        let parents = vec!["base".into()];
        let hashes = (0..256).map(|i| format!("stash{i}")).collect::<Vec<_>>();
        let (rows, cursor) = build(
            hashes
                .iter()
                .map(|h| (h.as_str(), parents.as_slice(), true)),
            &Cursor::default(),
        )
        .unwrap();
        assert!(rows.iter().all(|r| r.width == 2 && r.is_stash));
        assert_eq!(cursor.lanes.len(), 1);
        assert!(!cursor.lanes[0].is_stash);
        let (base, end) = build([("base", &[][..], false)], &cursor).unwrap();
        assert_eq!(base[0].node, 0);
        assert!(end.lanes.is_empty());
    }
}
