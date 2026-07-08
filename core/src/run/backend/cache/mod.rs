use alloc::{rc::Rc, vec::Vec};
use core::ops::Deref;

use crate::cpu::{Cpu, Exception, Instruction};

mod block;

#[derive(Default)]
pub struct CodeCache {
    blocks: block::PagedCache<CodeBlock>,
}

pub struct CodeBlock {
    pub ops: Vec<Result<Instruction, Exception>>,
}

pub struct CodeBlockHandle {
    inner: Rc<block::Cached<CodeBlock>>,
}

impl block::CacheEntry for CodeBlock {
    fn num_of_ops(&self) -> usize {
        self.ops.len()
    }
}

impl Deref for CodeBlockHandle {
    type Target = CodeBlock;

    fn deref(&self) -> &Self::Target {
        &self.inner.entry
    }
}

impl CodeCache {
    pub fn invalidate_all(&mut self) {
        self.blocks.invalidate_all();
    }

    pub fn invalidate_addr(&mut self, paddr: u32) {
        self.blocks.invalidate(paddr, None);
    }

    pub fn invalidate_block(&mut self, paddr: u32, handle: &CodeBlockHandle) -> bool {
        self.blocks.invalidate(paddr, Some(&*handle.inner))
    }

    pub fn insert(&mut self, paddr: u32, block: CodeBlock) -> CodeBlockHandle {
        CodeBlockHandle {
            inner: self.blocks.insert(paddr, block),
        }
    }

    pub fn get(&self, cpu: &Cpu) -> Option<CodeBlockHandle> {
        self.blocks.get(cpu).map(|inner| CodeBlockHandle { inner })
    }
}
