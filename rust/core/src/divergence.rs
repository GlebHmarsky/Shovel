//! Local-only comparison of immutable tips from a published history snapshot.
use crate::{
    git::{Error, Git, Snapshot},
    identity,
    process::Cancellation,
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Pending,
    Available {
        ahead: u64,
        behind: u64,
    },
    NoUpstream,
    MissingUpstream,
    Detached,
    Unborn,
    Shallow,
    Failed,
}
fn parse(bytes: &[u8]) -> Result<State, Error> {
    let text = std::str::from_utf8(bytes).map_err(|_| Error::Invalid)?;
    let values = text
        .trim_end_matches(['\r', '\n'])
        .split('\t')
        .collect::<Vec<_>>();
    if values.len() != 2 {
        return Err(Error::Invalid);
    }
    let number = |value: &str| {
        if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Error::Invalid);
        }
        value
            .parse::<u64>()
            .ok()
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or(Error::Invalid)
    };
    Ok(State::Available {
        ahead: number(values[0])?,
        behind: number(values[1])?,
    })
}
impl Git {
    pub fn divergence(&self, snapshot: &Snapshot, cancel: &Cancellation) -> Result<State, Error> {
        let Some(head) = &snapshot.head else {
            return Ok(State::Unborn);
        };
        if snapshot.branch.is_none() {
            return Ok(State::Detached);
        }
        let Some(upstream) = &snapshot.upstream else {
            return Ok(State::NoUpstream);
        };
        let Some(upstream) = snapshot.references.iter().find(|r| &r.name == upstream) else {
            return Ok(State::MissingUpstream);
        };
        if snapshot.shallow {
            return Ok(State::Shallow);
        }
        if [head, &upstream.hash]
            .iter()
            .any(|h| !matches!(h.len(), 40 | 64) || !h.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(Error::Invalid);
        }
        if identity::read(&snapshot.worktree)? != snapshot.identity {
            return Err(Error::Changed);
        }
        let range = format!("{head}...{}", upstream.hash);
        let output = self.read(
            &snapshot.worktree,
            ["rev-list", "--left-right", "--count", &range, "--"],
            cancel,
            4096,
        )?;
        if identity::read(&snapshot.worktree)? != snapshot.identity {
            return Err(Error::Changed);
        }
        parse(&output.stdout)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counts_distinguish_real_zero_and_reject_malformed_or_overflowed_output() {
        assert_eq!(
            parse(b"0\t0\n").unwrap(),
            State::Available {
                ahead: 0,
                behind: 0
            }
        );
        assert_eq!(
            parse(b"12\t34\r\n").unwrap(),
            State::Available {
                ahead: 12,
                behind: 34
            }
        );
        for value in [
            b"-1\t0".as_slice(),
            b"+1\t0",
            b"1 2",
            b"1\t2\t3",
            b"\t0",
            b"9223372036854775808\t0",
        ] {
            assert!(parse(value).is_err());
        }
    }
}
