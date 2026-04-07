/// Report of a pipeline execution.
#[derive(Debug)]
pub struct PipelineReport {
    pub completed: Vec<String>,
    pub skipped: Vec<String>,
    pub failed: Vec<(String, String)>,
    pub dry_run_entries: Vec<DryRunEntry>,
}

impl PipelineReport {
    pub fn new() -> Self {
        PipelineReport {
            completed: Vec::new(),
            skipped: Vec::new(),
            failed: Vec::new(),
            dry_run_entries: Vec::new(),
        }
    }

    pub fn completed(&mut self, name: &str) {
        self.completed.push(name.to_string());
    }

    pub fn skipped(&mut self, name: &str) {
        self.skipped.push(name.to_string());
    }

    pub fn failed(&mut self, name: &str, message: &str) {
        self.failed.push((name.to_string(), message.to_string()));
    }

    pub fn dry_run(&mut self, entry: DryRunEntry) {
        self.dry_run_entries.push(entry);
    }

    /// Print a human-readable summary.
    pub fn print_summary(&self) {
        for name in &self.completed {
            eprintln!("  ✓ {name}");
        }
        for name in &self.skipped {
            eprintln!("  ○ {name} (skipped)");
        }
        for (name, msg) in &self.failed {
            eprintln!("  ✗ {name}: {msg}");
        }
        for entry in &self.dry_run_entries {
            eprintln!("  ~ {} — {}", entry.pipe, entry.description);
        }
    }
}

impl Default for PipelineReport {
    fn default() -> Self {
        Self::new()
    }
}

/// A single dry-run entry describing what a pipe would do.
#[derive(Debug)]
pub struct DryRunEntry {
    pub pipe: String,
    pub description: String,
}
