//! Stable graph lanes across history pages.
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

pub(crate) fn build<'a>(
    commits: impl IntoIterator<Item = (&'a str, &'a [String])>,
    cursor: &Cursor,
) -> Result<(Vec<Row>, Cursor), GraphError> {
    let mut lanes = cursor.lanes.clone();
    let mut next_color = cursor.next_color;
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    if lanes.len() > MAX_LANES {
        return Err(GraphError::TooWide);
    }
    for (hash, parents) in commits {
        if parents.len() > MAX_LANES {
            return Err(GraphError::TooWide);
        }
        if !seen.insert(hash) || parents.iter().any(|p| seen.contains(p.as_str())) {
            return Err(GraphError::InvalidTopology);
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
            ("merge", vec!["left".into(), "right".into()]),
            ("left", vec!["base".into()]),
            ("right", vec!["base".into()]),
            ("base", vec![]),
        ];
        fn project(commits: &[(&str, Vec<String>)], cursor: &Cursor) -> (Vec<Row>, Cursor) {
            build(
                commits.iter().map(|(h, p)| (*h, p.as_slice())),
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
}
