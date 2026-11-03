//! The task graph a planner agent returns. Validated before it reaches the board.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Size {
    S,
    M,
    L,
    XL,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedTask {
    /// Planner-local id such as "t1", referenced by `depends_on`.
    pub id: String,
    pub title: String,
    pub description: String,
    pub acceptance: Vec<String>,
    #[serde(default)]
    pub predicted_files: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub size: Size,
    pub suggested_assignee: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskGraph {
    pub goal: String,
    pub tasks: Vec<PlannedTask>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PlanError {
    #[error("plan has no tasks")]
    Empty,
    #[error("plan has more than {0} tasks")]
    TooMany(usize),
    #[error("duplicate task id {0}")]
    DuplicateId(String),
    #[error("task {0} depends on unknown task {1}")]
    UnknownDependency(String, String),
    #[error("dependency cycle through task {0}")]
    Cycle(String),
    #[error("task {0} has an empty title")]
    EmptyTitle(String),
}

pub const MAX_TASKS: usize = 64;

/// JSON schema given to planner agents so they answer in this exact shape.
pub const TASK_GRAPH_SCHEMA: &str = r#"{
  "type": "object",
  "required": ["goal", "tasks"],
  "properties": {
    "goal": {"type": "string"},
    "tasks": {"type": "array", "maxItems": 64, "items": {
      "type": "object",
      "required": ["id", "title", "description", "acceptance", "size"],
      "properties": {
        "id": {"type": "string"},
        "title": {"type": "string"},
        "description": {"type": "string"},
        "acceptance": {"type": "array", "items": {"type": "string"}},
        "predicted_files": {"type": "array", "items": {"type": "string"}},
        "depends_on": {"type": "array", "items": {"type": "string"}},
        "size": {"enum": ["s", "m", "l", "xl"]},
        "suggested_assignee": {"type": ["string", "null"]}
      }
    }}
  }
}"#;

impl TaskGraph {
    pub fn validate(&self) -> Result<(), PlanError> {
        if self.tasks.is_empty() {
            return Err(PlanError::Empty);
        }
        if self.tasks.len() > MAX_TASKS {
            return Err(PlanError::TooMany(MAX_TASKS));
        }
        let mut ids = HashSet::new();
        for t in &self.tasks {
            if t.title.trim().is_empty() {
                return Err(PlanError::EmptyTitle(t.id.clone()));
            }
            if !ids.insert(t.id.as_str()) {
                return Err(PlanError::DuplicateId(t.id.clone()));
            }
        }
        for t in &self.tasks {
            for d in &t.depends_on {
                if !ids.contains(d.as_str()) {
                    return Err(PlanError::UnknownDependency(t.id.clone(), d.clone()));
                }
            }
        }
        self.topo_order().map(|_| ())
    }

    /// Tasks in dependency order.
    pub fn topo_order(&self) -> Result<Vec<&PlannedTask>, PlanError> {
        let by_id: HashMap<&str, &PlannedTask> = self.tasks.iter().map(|t| (t.id.as_str(), t)).collect();
        let mut state: HashMap<&str, u8> = HashMap::new();
        let mut out = Vec::new();
        fn visit<'a>(
            id: &'a str,
            by_id: &HashMap<&'a str, &'a PlannedTask>,
            state: &mut HashMap<&'a str, u8>,
            out: &mut Vec<&'a PlannedTask>,
        ) -> Result<(), PlanError> {
            match state.get(id) {
                Some(2) => return Ok(()),
                Some(1) => return Err(PlanError::Cycle(id.to_string())),
                _ => {}
            }
            state.insert(id, 1);
            if let Some(t) = by_id.get(id) {
                for d in &t.depends_on {
                    visit(d.as_str(), by_id, state, out)?;
                }
                out.push(t);
            }
            state.insert(id, 2);
            Ok(())
        }
        for t in &self.tasks {
            visit(t.id.as_str(), &by_id, &mut state, &mut out)?;
        }
        Ok(out)
    }

    /// Pairs of tasks whose predicted file sets intersect, with the shared files.
    pub fn predicted_overlaps(&self) -> Vec<(String, String, Vec<String>)> {
        let mut out = Vec::new();
        for (i, a) in self.tasks.iter().enumerate() {
            for b in &self.tasks[i + 1..] {
                let shared: Vec<String> =
                    a.predicted_files.iter().filter(|f| b.predicted_files.contains(f)).cloned().collect();
                if !shared.is_empty() {
                    out.push((a.id.clone(), b.id.clone(), shared));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, deps: &[&str], files: &[&str]) -> PlannedTask {
        PlannedTask {
            id: id.into(),
            title: format!("Task {id}"),
            description: String::new(),
            acceptance: vec![],
            predicted_files: files.iter().map(|s| s.to_string()).collect(),
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
            size: Size::M,
            suggested_assignee: None,
        }
    }

    #[test]
    fn validates_and_orders() {
        let g = TaskGraph {
            goal: "x".into(),
            tasks: vec![task("b", &["a"], &["x.rs"]), task("a", &[], &["x.rs", "y.rs"])],
        };
        g.validate().unwrap();
        let order: Vec<_> = g.topo_order().unwrap().iter().map(|t| t.id.clone()).collect();
        assert_eq!(order, vec!["a", "b"]);
        assert_eq!(g.predicted_overlaps(), vec![("b".into(), "a".into(), vec!["x.rs".into()])]);
    }

    #[test]
    fn rejects_cycles_and_unknown_deps() {
        let g = TaskGraph { goal: "x".into(), tasks: vec![task("a", &["b"], &[]), task("b", &["a"], &[])] };
        assert!(matches!(g.validate(), Err(PlanError::Cycle(_))));
        let g = TaskGraph { goal: "x".into(), tasks: vec![task("a", &["z"], &[])] };
        assert!(matches!(g.validate(), Err(PlanError::UnknownDependency(..))));
    }
}
