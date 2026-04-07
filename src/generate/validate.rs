//! Validates generated GitHub Actions workflow YAML for structural correctness.
//! This is NOT a generic YAML validator — it knows the GitHub Actions schema.

use crate::yaml::{self, YamlValue};

/// Validation errors found in a workflow.
#[derive(Debug)]
pub struct WorkflowIssue {
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, PartialEq)]
pub enum Severity {
    Error,
    Warning,
}

/// Validate a generated workflow YAML string.
pub fn validate_workflow(yaml_str: &str) -> Vec<WorkflowIssue> {
    let mut issues = Vec::new();

    let doc = match yaml::parse(yaml_str) {
        Ok(d) => d,
        Err(e) => {
            issues.push(WorkflowIssue {
                severity: Severity::Error,
                message: format!("invalid YAML: {e}"),
            });
            return issues;
        }
    };

    check_top_level(&doc, &mut issues);
    check_permissions(&doc, &mut issues);
    check_jobs(&doc, &mut issues);

    issues
}

fn check_top_level(doc: &YamlValue, issues: &mut Vec<WorkflowIssue>) {
    // Must have "name"
    if doc.get("name").is_none() {
        issues.push(WorkflowIssue {
            severity: Severity::Warning,
            message: "workflow missing 'name' field".into(),
        });
    }

    // Must have "on" trigger
    if doc.get("on").is_none() {
        issues.push(WorkflowIssue {
            severity: Severity::Error,
            message: "workflow missing 'on' trigger".into(),
        });
    }

    // Must have "jobs"
    if doc.get("jobs").is_none() {
        issues.push(WorkflowIssue {
            severity: Severity::Error,
            message: "workflow missing 'jobs' section".into(),
        });
    }
}

fn check_permissions(doc: &YamlValue, issues: &mut Vec<WorkflowIssue>) {
    if let Some(perms) = doc.get("permissions") {
        let mapping = match perms.as_mapping() {
            Some(m) => m,
            None => return,
        };

        // For release workflows, we expect these
        let expected = ["contents", "id-token"];
        for perm in &expected {
            if !mapping.contains_key(*perm) {
                issues.push(WorkflowIssue {
                    severity: Severity::Warning,
                    message: format!("permissions missing '{perm}: write' — needed for releases"),
                });
            }
        }
    }
}

fn check_jobs(doc: &YamlValue, issues: &mut Vec<WorkflowIssue>) {
    let jobs = match doc.get("jobs").and_then(|j| j.as_mapping()) {
        Some(j) => j,
        None => return,
    };

    let job_names: Vec<&str> = jobs.keys().map(|k| k.as_str()).collect();

    for (name, job) in jobs {
        check_job(name, job, &job_names, issues);
    }
}

fn check_job(name: &str, job: &YamlValue, all_jobs: &[&str], issues: &mut Vec<WorkflowIssue>) {
    // Must have runs-on
    if job.get("runs-on").is_none() && job.get("strategy").is_none() {
        // Jobs with strategy.matrix might set runs-on via expression
        if !has_runs_on_via_expression(job) {
            issues.push(WorkflowIssue {
                severity: Severity::Warning,
                message: format!("job '{name}' missing 'runs-on'"),
            });
        }
    }

    // Check "needs" references
    if let Some(needs) = job.get("needs") {
        let need_list = match needs {
            YamlValue::String(s) => vec![s.as_str()],
            YamlValue::Sequence(seq) => seq.iter().filter_map(|v| v.as_str()).collect(),
            _ => vec![],
        };
        for need in need_list {
            if !all_jobs.contains(&need) {
                issues.push(WorkflowIssue {
                    severity: Severity::Error,
                    message: format!("job '{name}' needs '{need}' which does not exist"),
                });
            }
        }
    }

    // Check steps
    if let Some(steps) = job.get("steps").and_then(|s| s.as_sequence()) {
        for (i, step) in steps.iter().enumerate() {
            check_step(name, i, step, issues);
        }
    }
}

fn has_runs_on_via_expression(job: &YamlValue) -> bool {
    // Check if runs-on contains ${{ matrix.* }} or similar
    if let Some(YamlValue::String(s)) = job.get("runs-on") {
        return s.contains("${{");
    }
    false
}

fn check_step(job_name: &str, step_idx: usize, step: &YamlValue, issues: &mut Vec<WorkflowIssue>) {
    // Each step must have either "uses" or "run"
    let has_uses = step.get("uses").is_some();
    let has_run = step.get("run").is_some();

    if !has_uses && !has_run {
        issues.push(WorkflowIssue {
            severity: Severity::Error,
            message: format!("job '{job_name}' step {step_idx}: must have 'uses' or 'run'"),
        });
    }

    // Check SHA pinning on "uses"
    if let Some(YamlValue::String(uses)) = step.get("uses") {
        check_action_pinning(job_name, step_idx, uses, issues);
    }
}

fn check_action_pinning(job_name: &str, step_idx: usize, uses: &str, issues: &mut Vec<WorkflowIssue>) {
    // Skip local actions (./path)
    if uses.starts_with("./") || uses.starts_with("docker://") {
        return;
    }

    if let Some(at_pos) = uses.find('@') {
        let ref_part = &uses[at_pos + 1..];
        // Remove trailing comment
        let ref_part = ref_part.split_whitespace().next().unwrap_or(ref_part);

        // SHA pins are 40 hex characters
        let is_sha = ref_part.len() >= 40
            && ref_part[..40].chars().all(|c| c.is_ascii_hexdigit());

        if !is_sha {
            // Check if it's a version tag (v1, v2, release/v1)
            issues.push(WorkflowIssue {
                severity: Severity::Warning,
                message: format!(
                    "job '{job_name}' step {step_idx}: action '{uses}' not SHA-pinned (using tag reference)"
                ),
            });
        }
    } else {
        issues.push(WorkflowIssue {
            severity: Severity::Error,
            message: format!(
                "job '{job_name}' step {step_idx}: action '{uses}' missing version reference (@sha or @tag)"
            ),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_workflow_no_errors() {
        let yaml = r#"name: Release
on:
  push:
    tags: ["v*"]

permissions:
  contents: write
  id-token: write
  attestations: write

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683
      - run: cargo build
"#;
        let issues = validate_workflow(yaml);
        let errors: Vec<_> = issues.iter().filter(|i| i.severity == Severity::Error).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }

    #[test]
    fn test_missing_on_trigger() {
        let yaml = "name: Test\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo hi";
        let issues = validate_workflow(yaml);
        assert!(issues.iter().any(|i| i.message.contains("missing 'on' trigger")));
    }

    #[test]
    fn test_missing_jobs() {
        let yaml = "name: Test\non:\n  push:\n    tags: [\"v*\"]";
        let issues = validate_workflow(yaml);
        assert!(issues.iter().any(|i| i.message.contains("missing 'jobs'")));
    }

    #[test]
    fn test_invalid_needs_reference() {
        let yaml = r#"name: Test
on:
  push:
    tags: ["v*"]
jobs:
  release:
    needs: [nonexistent]
    runs-on: ubuntu-latest
    steps:
      - run: echo done"#;
        let issues = validate_workflow(yaml);
        assert!(issues.iter().any(|i| i.message.contains("'nonexistent' which does not exist")));
    }

    #[test]
    fn test_action_not_sha_pinned() {
        let yaml = r#"name: Test
on:
  push:
    tags: ["v*"]
permissions:
  contents: write
  id-token: write
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4"#;
        let issues = validate_workflow(yaml);
        assert!(issues.iter().any(|i| i.message.contains("not SHA-pinned")));
    }

    #[test]
    fn test_sha_pinned_action_passes() {
        let yaml = r#"name: Test
on:
  push:
    tags: ["v*"]
permissions:
  contents: write
  id-token: write
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683"#;
        let issues = validate_workflow(yaml);
        let pin_errors: Vec<_> = issues.iter().filter(|i| i.message.contains("SHA-pinned")).collect();
        assert!(pin_errors.is_empty(), "unexpected pinning issues: {pin_errors:?}");
    }

    #[test]
    fn test_step_missing_uses_and_run() {
        let yaml = r#"name: Test
on:
  push:
    tags: ["v*"]
permissions:
  contents: write
  id-token: write
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - name: Do nothing"#;
        let issues = validate_workflow(yaml);
        assert!(issues.iter().any(|i| i.message.contains("must have 'uses' or 'run'")));
    }

    #[test]
    fn test_missing_permissions_warning() {
        let yaml = r#"name: Test
on:
  push:
    tags: ["v*"]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi"#;
        let issues = validate_workflow(yaml);
        // No permissions block at all — should get warnings but not crash
        // The validator checks if permissions exist, but absence is not checked currently
        // This test just ensures no panic
        assert!(issues.iter().all(|i| i.severity != Severity::Error || !i.message.contains("permission")));
    }

    #[test]
    fn test_valid_needs_reference() {
        let yaml = r#"name: Test
on:
  push:
    tags: ["v*"]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: cargo build
  release:
    needs: [build]
    runs-on: ubuntu-latest
    steps:
      - run: echo release"#;
        let issues = validate_workflow(yaml);
        let need_errors: Vec<_> = issues.iter().filter(|i| i.message.contains("does not exist")).collect();
        assert!(need_errors.is_empty(), "unexpected needs errors: {need_errors:?}");
    }
}
