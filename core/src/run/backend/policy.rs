use crate::SystemTime;

pub trait ExecutionPolicy {
    fn select(&mut self, context: PolicyContext) -> ExecutionMode;
}

#[derive(Clone, Copy, Debug)]
pub struct PolicyContext {
    /// Remaining budget for an execution step.
    pub budget: SystemTime,
    /// Previous invocations of the block; zero on a cache miss.
    pub hits: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionMode {
    Interpreter,
    CachedInterpreter,
}

impl ExecutionPolicy for ExecutionMode {
    fn select(&mut self, _: PolicyContext) -> ExecutionMode {
        *self
    }
}

#[derive(Default)]
pub struct DefaultPolicy;

impl ExecutionPolicy for DefaultPolicy {
    fn select(&mut self, context: PolicyContext) -> ExecutionMode {
        if context.budget < 8 {
            ExecutionMode::Interpreter
        } else {
            ExecutionMode::CachedInterpreter
        }
    }
}
