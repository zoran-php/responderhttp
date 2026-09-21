// http_client/src-tauri/src/openapi/import/grouping.rs
//
// How imported requests are arranged into folders (PLAN.md Phase 8d,
// decision D1). The dialog offers all three; `default_grouping` picks the one
// it opens with.
//
// - Tags: one folder per tag. An operation with several tags goes into the
//   first one only — copies in every folder would drift apart the first time
//   one is edited. 3.2's Tag `parent` nests the folders.
// - Paths: one folder per URL segment, template segments left out, and a
//   chain of folders that only lead to one other folder merged into one.
// - Flat: everything at the collection root.
use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::domain::import_plan::PlannedFolder;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grouping {
    Tags,
    Paths,
    Flat,
}

impl Grouping {
    pub const ALL: [Grouping; 3] = [Grouping::Tags, Grouping::Paths, Grouping::Flat];

    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "tags" => Some(Self::Tags),
            "paths" => Some(Self::Paths),
            "flat" => Some(Self::Flat),
            _ => None,
        }
    }

    pub fn wire(self) -> &'static str {
        match self {
            Self::Tags => "tags",
            Self::Paths => "paths",
            Self::Flat => "flat",
        }
    }
}

/// What grouping needs to know about one operation.
pub struct Placement<'a> {
    pub path: &'a str,
    pub tags: Vec<&'a str>,
}

/// A declared tag, from the document's top-level `tags`.
pub struct DeclaredTag<'a> {
    pub name: &'a str,
    pub parent: Option<&'a str>,
}

/// The folders to create and, per operation, the folder it goes into.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Arrangement {
    pub folders: Vec<PlannedFolder>,
    pub assignment: Vec<Option<usize>>,
    /// Tags whose `parent` could not be honoured (unknown, or a loop).
    pub unnested_tags: Vec<String>,
}

impl Arrangement {
    pub fn top_level_names(&self) -> Vec<String> {
        self.folders
            .iter()
            .filter(|folder| folder.parent.is_none())
            .map(|folder| folder.name.clone())
            .collect()
    }
}

/// Tags when the author tagged anything, paths otherwise.
pub fn default_grouping(placements: &[Placement<'_>]) -> Grouping {
    if placements
        .iter()
        .any(|placement| !placement.tags.is_empty())
    {
        Grouping::Tags
    } else {
        Grouping::Paths
    }
}

pub fn arrange(
    grouping: Grouping,
    placements: &[Placement<'_>],
    declared: &[DeclaredTag<'_>],
) -> Arrangement {
    match grouping {
        Grouping::Tags => by_tags(placements, declared),
        Grouping::Paths => by_paths(placements),
        Grouping::Flat => Arrangement {
            assignment: vec![None; placements.len()],
            ..Arrangement::default()
        },
    }
}

/// The declared tags from a document's top-level `tags` list. `parent` only
/// exists from 3.2; an older document simply has none.
pub fn declared_tags(document: &Value) -> Vec<DeclaredTag<'_>> {
    document
        .get("tags")
        .and_then(Value::as_array)
        .map(|tags| {
            tags.iter()
                .filter_map(|tag| {
                    Some(DeclaredTag {
                        name: tag.get("name")?.as_str()?,
                        parent: tag.get("parent").and_then(Value::as_str),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn by_tags(placements: &[Placement<'_>], declared: &[DeclaredTag<'_>]) -> Arrangement {
    // Declared order first, then the order undeclared tags first appear in.
    let mut order: Vec<&str> = Vec::new();
    let mut parents: BTreeMap<&str, Option<&str>> = BTreeMap::new();
    for tag in declared {
        if !parents.contains_key(tag.name) {
            order.push(tag.name);
            parents.insert(tag.name, tag.parent);
        }
    }
    for placement in placements {
        if let Some(first) = placement.tags.first() {
            if !parents.contains_key(first) {
                order.push(first);
                parents.insert(first, None);
            }
        }
    }

    let mut arrangement = Arrangement::default();
    let mut index_of: BTreeMap<&str, usize> = BTreeMap::new();
    for name in &order {
        ensure_tag_folder(name, &parents, &mut index_of, &mut arrangement);
    }
    arrangement.assignment = placements
        .iter()
        .map(|placement| {
            placement
                .tags
                .first()
                .and_then(|tag| index_of.get(tag).copied())
        })
        .collect();
    arrangement
}

/// Creates the folder for `name`, its parents first, so every folder's
/// parent index is lower than its own.
fn ensure_tag_folder<'a>(
    name: &'a str,
    parents: &BTreeMap<&'a str, Option<&'a str>>,
    index_of: &mut BTreeMap<&'a str, usize>,
    arrangement: &mut Arrangement,
) -> usize {
    if let Some(index) = index_of.get(name) {
        return *index;
    }
    let parent = match nestable_parent(name, parents) {
        Ok(parent) => {
            parent.map(|parent| ensure_tag_folder(parent, parents, index_of, arrangement))
        }
        Err(()) => {
            arrangement.unnested_tags.push(name.to_string());
            None
        }
    };
    let index = arrangement.folders.len();
    arrangement.folders.push(PlannedFolder {
        name: name.to_string(),
        parent,
    });
    index_of.insert(name, index);
    index
}

/// `Err` when the parent is unknown or the chain above `name` loops.
fn nestable_parent<'a>(
    name: &'a str,
    parents: &BTreeMap<&'a str, Option<&'a str>>,
) -> Result<Option<&'a str>, ()> {
    let Some(parent) = parents.get(name).copied().flatten() else {
        return Ok(None);
    };
    let mut seen = BTreeSet::from([name]);
    let mut current = parent;
    loop {
        if !seen.insert(current) {
            return Err(());
        }
        match parents.get(current).copied() {
            None => return Err(()),
            Some(None) => return Ok(Some(parent)),
            Some(Some(next)) => current = next,
        }
    }
}

#[derive(Default)]
struct Node {
    name: String,
    children: Vec<usize>,
    child_by_name: BTreeMap<String, usize>,
    operations: Vec<usize>,
}

fn by_paths(placements: &[Placement<'_>]) -> Arrangement {
    // A trie of literal segments; node 0 is the collection root.
    let mut nodes = vec![Node::default()];
    for (operation, placement) in placements.iter().enumerate() {
        let mut current = 0;
        for segment in literal_segments(placement.path) {
            current = match nodes[current].child_by_name.get(segment) {
                Some(existing) => *existing,
                None => {
                    let created = nodes.len();
                    nodes.push(Node {
                        name: segment.to_string(),
                        ..Node::default()
                    });
                    nodes[current].children.push(created);
                    nodes[current]
                        .child_by_name
                        .insert(segment.to_string(), created);
                    created
                }
            };
        }
        nodes[current].operations.push(operation);
    }

    let mut arrangement = Arrangement {
        assignment: vec![None; placements.len()],
        ..Arrangement::default()
    };
    // Operations on `/` itself stay at the root, as `assignment` already says.
    for &child in &nodes[0].children {
        emit(&nodes, child, None, &mut arrangement);
    }
    arrangement
}

fn emit(nodes: &[Node], start: usize, parent: Option<usize>, arrangement: &mut Arrangement) {
    // Folders that hold nothing but a single sub-folder are merged into it.
    let mut name = nodes[start].name.clone();
    let mut current = start;
    while nodes[current].operations.is_empty() && nodes[current].children.len() == 1 {
        current = nodes[current].children[0];
        name = format!("{name}/{}", nodes[current].name);
    }

    let index = arrangement.folders.len();
    arrangement.folders.push(PlannedFolder { name, parent });
    for &operation in &nodes[current].operations {
        arrangement.assignment[operation] = Some(index);
    }
    for &child in &nodes[current].children {
        emit(nodes, child, Some(index), arrangement);
    }
}

/// `/repos/{owner}/{repo}/issues` → `repos`, `issues`.
fn literal_segments(path: &str) -> impl Iterator<Item = &str> {
    path.split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .filter(|segment| !(segment.starts_with('{') && segment.ends_with('}')))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn placement<'a>(path: &'a str, tags: &[&'a str]) -> Placement<'a> {
        Placement {
            path,
            tags: tags.to_vec(),
        }
    }

    fn folder(name: &str, parent: Option<usize>) -> PlannedFolder {
        PlannedFolder {
            name: name.into(),
            parent,
        }
    }

    #[test]
    fn tags_follow_declared_order_then_first_use() {
        let placements = [
            placement("/b", &["beta"]),
            placement("/x", &["extra"]),
            placement("/a", &["alpha", "beta"]),
            placement("/none", &[]),
        ];
        let declared = [
            DeclaredTag {
                name: "alpha",
                parent: None,
            },
            DeclaredTag {
                name: "beta",
                parent: None,
            },
            DeclaredTag {
                name: "unused",
                parent: None,
            },
        ];

        let arrangement = arrange(Grouping::Tags, &placements, &declared);

        assert_eq!(
            arrangement.folders,
            vec![
                folder("alpha", None),
                folder("beta", None),
                folder("unused", None),
                folder("extra", None)
            ]
        );
        assert_eq!(
            arrangement.assignment,
            vec![Some(1), Some(3), Some(0), None]
        );
    }

    #[test]
    fn tag_parents_nest_with_parents_created_first() {
        let placements = [placement("/c", &["child"])];
        let declared = [
            DeclaredTag {
                name: "child",
                parent: Some("middle"),
            },
            DeclaredTag {
                name: "middle",
                parent: Some("top"),
            },
            DeclaredTag {
                name: "top",
                parent: None,
            },
        ];

        let arrangement = arrange(Grouping::Tags, &placements, &declared);

        assert_eq!(
            arrangement.folders,
            vec![
                folder("top", None),
                folder("middle", Some(0)),
                folder("child", Some(1))
            ]
        );
        assert_eq!(arrangement.assignment, vec![Some(2)]);
        assert!(arrangement.unnested_tags.is_empty());
    }

    #[test]
    fn a_parent_loop_or_unknown_parent_is_flattened_and_reported() {
        let declared = [
            DeclaredTag {
                name: "a",
                parent: Some("b"),
            },
            DeclaredTag {
                name: "b",
                parent: Some("a"),
            },
            DeclaredTag {
                name: "c",
                parent: Some("missing"),
            },
            DeclaredTag {
                name: "d",
                parent: Some("d"),
            },
        ];

        let arrangement = arrange(Grouping::Tags, &[], &declared);

        assert!(arrangement.folders.iter().all(|f| f.parent.is_none()));
        assert_eq!(arrangement.unnested_tags, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn paths_become_folders_without_template_segments() {
        let placements = [
            placement("/pets", &[]),
            placement("/pets/{id}", &[]),
            placement("/repos/{owner}/{repo}/issues", &[]),
            placement("/repos/{owner}/{repo}/pulls", &[]),
            placement("/", &[]),
        ];

        let arrangement = arrange(Grouping::Paths, &placements, &[]);

        assert_eq!(
            arrangement.folders,
            vec![
                folder("pets", None),
                folder("repos", None),
                folder("issues", Some(1)),
                folder("pulls", Some(1))
            ]
        );
        assert_eq!(
            arrangement.assignment,
            vec![Some(0), Some(0), Some(2), Some(3), None]
        );
    }

    #[test]
    fn a_chain_of_single_folders_is_merged() {
        let placements = [
            placement("/api/v1/users", &[]),
            placement("/api/v1/users/{id}/orders", &[]),
        ];

        let arrangement = arrange(Grouping::Paths, &placements, &[]);

        assert_eq!(
            arrangement.folders,
            vec![folder("api/v1/users", None), folder("orders", Some(0))]
        );
        assert_eq!(arrangement.assignment, vec![Some(0), Some(1)]);
    }

    #[test]
    fn flat_puts_everything_at_the_root() {
        let placements = [placement("/a", &["x"]), placement("/b/c", &[])];

        let arrangement = arrange(Grouping::Flat, &placements, &[]);

        assert!(arrangement.folders.is_empty());
        assert_eq!(arrangement.assignment, vec![None, None]);
    }

    #[test]
    fn the_default_is_tags_only_when_something_is_tagged() {
        assert_eq!(
            default_grouping(&[placement("/a", &[]), placement("/b", &["t"])]),
            Grouping::Tags
        );
        assert_eq!(default_grouping(&[placement("/a", &[])]), Grouping::Paths);
    }

    #[test]
    fn declared_tags_are_read_with_their_parents() {
        let document = json!({"tags": [
            {"name": "a"},
            {"name": "b", "parent": "a", "kind": "nav"},
            {"description": "no name"}
        ]});

        let tags = declared_tags(&document);

        assert_eq!(
            tags.iter().map(|t| (t.name, t.parent)).collect::<Vec<_>>(),
            vec![("a", None), ("b", Some("a"))]
        );
    }

    #[test]
    fn grouping_names_round_trip() {
        for grouping in Grouping::ALL {
            assert_eq!(Grouping::from_wire(grouping.wire()), Some(grouping));
        }
        assert_eq!(Grouping::from_wire("folders"), None);
    }
}
