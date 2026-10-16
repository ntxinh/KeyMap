use std::collections::HashMap;

use crate::model::{Binding, KeyCombo, Source};

#[derive(Debug, PartialEq)]
pub enum Kind {
    Exact,
    FishPrefix,
}

#[derive(Debug)]
pub struct Conflict {
    pub combo: KeyCombo,
    pub bindings: Vec<Binding>,
    pub kind: Kind,
}

pub fn find_conflicts(bindings: &[Binding]) -> Vec<Conflict> {
    // Exact: whole-combo equality (sequence included), keep groups >1.
    let mut groups: HashMap<&KeyCombo, Vec<&Binding>> = HashMap::new();
    for b in bindings {
        groups.entry(&b.combo).or_default().push(b);
    }
    let mut conflicts: Vec<Conflict> = groups
        .values()
        .filter(|g| g.len() > 1)
        .map(|g| Conflict {
            combo: g[0].combo.clone(),
            bindings: g.iter().map(|b| (*b).clone()).collect(),
            kind: Kind::Exact,
        })
        .collect();

    // FishPrefix: a fish sequence's head (mods+key, tail stripped) colliding
    // with a non-fish binding's full combo shadows that binding.
    for fish in bindings
        .iter()
        .filter(|b| b.source == Source::Fish && b.combo.sequence.is_some())
    {
        let head = KeyCombo {
            mods: fish.combo.mods,
            key: fish.combo.key.clone(),
            sequence: None,
        };
        // one report per fish binding ⇒ no duplicate (fish, non-fish) pairs
        let mut hits = vec![fish.clone()];
        hits.extend(
            bindings
                .iter()
                .filter(|b| b.source != Source::Fish && b.combo == head)
                .cloned(),
        );
        if hits.len() > 1 {
            conflicts.push(Conflict {
                combo: fish.combo.clone(),
                bindings: hits,
                kind: Kind::FishPrefix,
            });
        }
    }
    conflicts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Context, Mods};
    use std::path::PathBuf;

    fn binding(source: Source, mods: Mods, key: &str, seq: Option<KeyCombo>) -> Binding {
        Binding {
            combo: KeyCombo { mods, key: key.into(), sequence: seq.map(Box::new) },
            raw: String::new(),
            action: "act".into(),
            description: None,
            source,
            origin: PathBuf::new(),
            is_default: false,
            context: Context::Global,
        }
    }

    fn fish_seq() -> KeyCombo {
        KeyCombo { mods: Mods::NONE, key: "b".into(), sequence: None }
    }

    #[test]
    fn exact_cross_source() {
        let bs = vec![
            binding(Source::Niri, Mods::CTRL, "x", None),
            binding(Source::Ghostty, Mods::CTRL, "x", None),
        ];
        let cs = find_conflicts(&bs);
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].kind, Kind::Exact);
        assert_eq!(cs[0].bindings.len(), 2);
    }

    #[test]
    fn fish_prefix_collision() {
        let bs = vec![
            binding(Source::Fish, Mods::CTRL, "x", Some(fish_seq())),
            binding(Source::Niri, Mods::CTRL, "x", None),
        ];
        let cs = find_conflicts(&bs);
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].kind, Kind::FishPrefix);
        assert_eq!(cs[0].bindings.len(), 2);
    }

    #[test]
    fn distinct_combos_no_conflicts() {
        let bs = vec![
            binding(Source::Niri, Mods::CTRL, "x", None),
            binding(Source::Ghostty, Mods::ALT, "y", None),
            binding(Source::Fish, Mods::CTRL, "z", Some(fish_seq())),
        ];
        assert!(find_conflicts(&bs).is_empty());
    }

    #[test]
    fn fish_sequences_same_head_different_tails_not_exact() {
        let tail_b = fish_seq();
        let tail_c = KeyCombo { mods: Mods::NONE, key: "c".into(), sequence: None };
        let bs = vec![
            binding(Source::Fish, Mods::CTRL, "x", Some(tail_b)),
            binding(Source::Fish, Mods::CTRL, "x", Some(tail_c)),
        ];
        assert!(find_conflicts(&bs).is_empty());
    }
}
