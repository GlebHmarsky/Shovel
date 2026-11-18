//! Port of the baseline's stable, paged graph. Stashes annotate their actual
//! base without taking ownership of an ordinary branch lane.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_LANES: usize = 64;
pub const MAX_DISPLAY_LANES: usize = MAX_LANES + 1;
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Cursor {
    pub lanes: Vec<Lane>,
    pub next_color: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Lane {
    pub hash: String,
    pub color: usize,
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
    /// Commit lane. A stash marker is inset to the right of its base's lane.
    pub node: usize,
    pub color: usize,
    pub width: usize,
    pub is_merge: bool,
    pub stash_link: Option<StashLink>,
    pub lines: Vec<Line>,
}
impl Row {
    pub fn is_stash(&self) -> bool {
        matches!(self.stash_link, Some(StashLink::Leaf { .. }))
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum StashLink {
    Leaf { first: bool, shared: bool },
    Base { shared: bool },
}

#[derive(Clone, Copy)]
pub(crate) enum Placement {
    Commit { stashes: usize },
    Stash { index: usize, count: usize },
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

pub(crate) fn build<'a>(
    commits: impl IntoIterator<Item = (&'a str, &'a [String], Placement)>,
    cursor: &Cursor,
) -> Result<(Vec<Row>, Cursor), GraphError> {
    let mut lanes = cursor.lanes.clone();
    let mut next_color = cursor.next_color;
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    if lanes.len() > MAX_LANES {
        return Err(GraphError::TooWide);
    }
    for (hash, parents, placement) in commits {
        if parents.len() > MAX_LANES {
            return Err(GraphError::TooWide);
        }
        if !seen.insert(hash) || parents.iter().any(|p| seen.contains(p.as_str())) {
            return Err(GraphError::InvalidTopology);
        }
        if let Placement::Stash { index, count } = placement {
            let [base] = parents else {
                return Err(GraphError::InvalidTopology);
            };
            let lines = lanes
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
            // Predict the base's lane without reserving it or advancing branch colors.
            let node = lanes
                .iter()
                .position(|lane| lane.hash == *base)
                .or_else(|| lanes.iter().position(|lane| lane.hash.is_empty()))
                .unwrap_or(lanes.len());
            if node >= MAX_LANES {
                return Err(GraphError::TooWide);
            }
            rows.push(Row {
                node,
                color: 0,
                width: lanes.len().max(node + 2),
                is_merge: false,
                stash_link: Some(StashLink::Leaf {
                    first: index == 0,
                    shared: count > 1,
                }),
                lines,
            });
            continue;
        }
        let existing = lanes.iter().position(|l| l.hash == hash);
        let node = match existing {
            Some(i) => i,
            None => {
                let i = allocate(
                    &mut lanes,
                    Lane {
                        hash: hash.into(),
                        color: next_color,
                    },
                )?;
                next_color += 1;
                i
            }
        };
        let before = lanes.clone();
        let stash_link = match placement {
            Placement::Commit { stashes } if stashes > 0 => {
                Some(StashLink::Base { shared: stashes > 1 })
            }
            _ => None,
        };
        let color = before[node].color;
        for lane in &mut lanes {
            if lane.hash == hash {
                lane.hash.clear();
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
                            },
                        )?;
                        next_color += 1;
                        i
                    }
                };
                outgoing.push(target);
            }
        }
        let width = before
            .len()
            .max(lanes.len())
            .max(node + 1 + usize::from(stash_link.is_some()));
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
            stash_link,
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
    const COMMIT: Placement = Placement::Commit { stashes: 0 };

    #[test]
    fn page_boundaries_preserve_graph_exactly() {
        let commits = [
            ("merge", vec!["left".into(), "right".into()]),
            ("left", vec!["base".into()]),
            ("right", vec!["base".into()]),
            ("base", vec![]),
        ];
        fn project(commits: &[(&str, Vec<String>)], cursor: &Cursor) -> (Vec<Row>, Cursor) {
            build(
                commits.iter().map(|(h, p)| (*h, p.as_slice(), COMMIT)),
                cursor,
            )
            .unwrap()
        }
        let (all, end) = project(&commits, &Cursor::default());
        let (mut first, cursor) = project(&commits[..2], &Cursor::default());
        let (last, split_end) = project(&commits[2..], &cursor);
        first.extend(last);
        assert_eq!(all, first);
        assert_eq!(end, split_end);
        assert!(end.lanes.is_empty());
    }
    #[test]
    fn many_stashes_share_a_rail_without_reserving_commit_lanes() {
        let parents = vec!["base".into()];
        let hashes = (0..256).map(|i| format!("stash{i}")).collect::<Vec<_>>();
        let (rows, cursor) = build(
            hashes.iter().enumerate().map(|(index, h)| {
                (
                    h.as_str(),
                    parents.as_slice(),
                    Placement::Stash {
                        index,
                        count: hashes.len(),
                    },
                )
            }),
            &Cursor::default(),
        )
        .unwrap();
        for (index, row) in rows.iter().enumerate() {
            assert_eq!(
                row.stash_link,
                Some(StashLink::Leaf {
                    first: index == 0,
                    shared: true
                })
            );
            assert_eq!((row.node, row.width), (0, 2));
            assert!(row.lines.is_empty());
        }
        assert_eq!(cursor, Cursor::default());
        let (base, end) = build(
            [(
                "base",
                &[][..],
                Placement::Commit {
                    stashes: hashes.len(),
                },
            )],
            &cursor,
        )
        .unwrap();
        assert_eq!(base[0].node, 0);
        assert_eq!(
            base[0].stash_link,
            Some(StashLink::Base { shared: true })
        );
        assert_eq!(end.next_color, 1);
        assert!(end.lanes.is_empty());
    }

    #[test]
    fn stash_leaves_keep_commit_edges_intact_when_two_tips_share_a_parent() {
        let parents = ["common".to_owned()];
        let ordinary = [
            ("current", &parents[..], COMMIT),
            ("previous", &parents[..], COMMIT),
            ("common", &[][..], COMMIT),
        ];
        let stash_parents = ["previous".to_owned()];
        let with_stash = [
            ordinary[0],
            (
                "stash",
                &stash_parents[..],
                Placement::Stash { index: 0, count: 1 },
            ),
            ("previous", &parents[..], Placement::Commit { stashes: 1 }),
            ordinary[2],
        ];
        let (expected, expected_cursor) = build(ordinary, &Cursor::default()).unwrap();
        let (mut rows, cursor) = build(with_stash, &Cursor::default()).unwrap();
        let stash = rows.remove(1);
        assert_eq!(stash.node, rows[1].node);
        assert_eq!(
            stash.lines,
            vec![Line {
                from: 0,
                from_y: 0.,
                to: 0,
                to_y: 1.,
                color: 0
            }]
        );
        assert_eq!(
            rows[1].stash_link.take(),
            Some(StashLink::Base { shared: false })
        );
        rows[1].width -= 1;
        assert_eq!(rows, expected);
        assert_eq!(cursor, expected_cursor);
    }
}
