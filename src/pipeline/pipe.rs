use super::context::Context;
use super::report::DryRunEntry;

/// A single unit of work in the pipeline.
pub trait Pipe {
    /// Human-readable name for logging and reports.
    fn name(&self) -> &str;

    /// Return true to skip this pipe (e.g., channel not enabled).
    fn skip(&self, _ctx: &Context) -> bool {
        false
    }

    /// Execute the pipe's work, potentially mutating context.
    fn run(&self, ctx: &mut Context) -> Result<(), String>;

    /// Describe what this pipe would do without doing it.
    fn dry_run(&self, _ctx: &Context) -> DryRunEntry {
        DryRunEntry {
            pipe: self.name().to_string(),
            description: format!("would execute '{}'", self.name()),
        }
    }
}
