//! Pipeline-of-pipes execution model.
//! Each pipe is a discrete, testable unit of work.
//! The pipeline runner sequences pipes, handles skip/error/dry-run.

mod context;
mod pipe;
mod report;

pub use context::{Artifact, ArtifactKind, Context};
pub use pipe::Pipe;
pub use report::{DryRunEntry, PipelineReport};

/// A pipeline is a linear sequence of pipes.
pub struct Pipeline {
    pipes: Vec<Box<dyn Pipe>>,
}

impl Pipeline {
    pub fn new() -> Self {
        Pipeline { pipes: Vec::new() }
    }

    pub fn push(mut self, pipe: Box<dyn Pipe>) -> Self {
        self.pipes.push(pipe);
        self
    }

    /// Add a pipe only if the condition is true.
    pub fn push_if(self, condition: bool, pipe: Box<dyn Pipe>) -> Self {
        if condition {
            self.push(pipe)
        } else {
            self
        }
    }

    /// Execute all pipes in sequence.
    pub fn execute(&self, ctx: &mut Context) -> Result<PipelineReport, Box<PipelineError>> {
        let mut report = PipelineReport::new();

        for pipe in &self.pipes {
            let name = pipe.name().to_string();

            if pipe.skip(ctx) {
                report.skipped(&name);
                continue;
            }

            if ctx.dry_run {
                let entry = pipe.dry_run(ctx);
                report.dry_run(entry);
                continue;
            }

            match pipe.run(ctx) {
                Ok(()) => {
                    report.completed(&name);
                }
                Err(e) => {
                    report.failed(&name, &e);
                    return Err(Box::new(PipelineError {
                        pipe: name,
                        message: e,
                        report,
                    }));
                }
            }
        }

        Ok(report)
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

/// Error from a pipeline execution, includes partial report.
#[derive(Debug)]
pub struct PipelineError {
    pub pipe: String,
    pub message: String,
    pub report: PipelineReport,
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pipeline failed at '{}': {}", self.pipe, self.message)
    }
}

impl std::error::Error for PipelineError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// A mock pipe that records whether it was called.
    struct MockPipe {
        name: String,
        calls: Arc<Mutex<Vec<String>>>,
        should_skip: bool,
        should_fail: Option<String>,
    }

    impl MockPipe {
        fn new(name: &str, calls: Arc<Mutex<Vec<String>>>) -> Self {
            MockPipe {
                name: name.into(),
                calls,
                should_skip: false,
                should_fail: None,
            }
        }

        fn skipping(mut self) -> Self {
            self.should_skip = true;
            self
        }

        fn failing(mut self, msg: &str) -> Self {
            self.should_fail = Some(msg.into());
            self
        }
    }

    impl Pipe for MockPipe {
        fn name(&self) -> &str {
            &self.name
        }

        fn skip(&self, _ctx: &Context) -> bool {
            self.should_skip
        }

        fn run(&self, _ctx: &mut Context) -> Result<(), String> {
            self.calls.lock().unwrap().push(self.name.clone());
            if let Some(msg) = &self.should_fail {
                Err(msg.clone())
            } else {
                Ok(())
            }
        }

        fn dry_run(&self, _ctx: &Context) -> DryRunEntry {
            DryRunEntry {
                pipe: self.name.clone(),
                description: format!("would run {}", self.name),
            }
        }
    }

    fn test_context() -> Context {
        Context::new_dry_run(false)
    }

    #[test]
    fn test_pipeline_executes_in_order() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let pipeline = Pipeline::new()
            .push(Box::new(MockPipe::new("first", calls.clone())))
            .push(Box::new(MockPipe::new("second", calls.clone())))
            .push(Box::new(MockPipe::new("third", calls.clone())));

        let mut ctx = test_context();
        let report = pipeline.execute(&mut ctx).unwrap();

        assert_eq!(*calls.lock().unwrap(), vec!["first", "second", "third"]);
        assert_eq!(report.completed.len(), 3);
    }

    #[test]
    fn test_pipeline_skips_pipe() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let pipeline = Pipeline::new()
            .push(Box::new(MockPipe::new("first", calls.clone())))
            .push(Box::new(MockPipe::new("skip-me", calls.clone()).skipping()))
            .push(Box::new(MockPipe::new("third", calls.clone())));

        let mut ctx = test_context();
        let report = pipeline.execute(&mut ctx).unwrap();

        assert_eq!(*calls.lock().unwrap(), vec!["first", "third"]);
        assert_eq!(report.completed.len(), 2);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0], "skip-me");
    }

    #[test]
    fn test_pipeline_stops_on_error() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let pipeline = Pipeline::new()
            .push(Box::new(MockPipe::new("first", calls.clone())))
            .push(Box::new(MockPipe::new("boom", calls.clone()).failing("kaboom")))
            .push(Box::new(MockPipe::new("never", calls.clone())));

        let mut ctx = test_context();
        let err = pipeline.execute(&mut ctx).unwrap_err();

        assert_eq!(err.pipe, "boom");
        assert_eq!(err.message, "kaboom");
        assert_eq!(*calls.lock().unwrap(), vec!["first", "boom"]);
        // "never" was not called
        assert_eq!(err.report.completed.len(), 1);
    }

    #[test]
    fn test_pipeline_dry_run() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let pipeline = Pipeline::new()
            .push(Box::new(MockPipe::new("first", calls.clone())))
            .push(Box::new(MockPipe::new("second", calls.clone())));

        let mut ctx = Context::new_dry_run(true);
        let report = pipeline.execute(&mut ctx).unwrap();

        // No pipes were actually executed
        assert!(calls.lock().unwrap().is_empty());
        assert_eq!(report.dry_run_entries.len(), 2);
    }

    #[test]
    fn test_pipeline_add_if() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let pipeline = Pipeline::new()
            .push(Box::new(MockPipe::new("always", calls.clone())))
            .push_if(false, Box::new(MockPipe::new("never", calls.clone())))
            .push_if(true, Box::new(MockPipe::new("conditionally", calls.clone())));

        let mut ctx = test_context();
        pipeline.execute(&mut ctx).unwrap();

        assert_eq!(*calls.lock().unwrap(), vec!["always", "conditionally"]);
    }

    #[test]
    fn test_empty_pipeline() {
        let pipeline = Pipeline::new();
        let mut ctx = test_context();
        let report = pipeline.execute(&mut ctx).unwrap();
        assert!(report.completed.is_empty());
    }
}
